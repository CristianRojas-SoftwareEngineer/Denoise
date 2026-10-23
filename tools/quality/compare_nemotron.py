"""Comparación de calidad y rendimiento CPU: NVIDIA NemotronLabs RE-USE vs DPDFNet.

Evalúa:
1. Velocidad CPU-only (tiempo de ejecución y RTF = pared / audio).
2. Calidad del audio de salida (ganancia SI-SDR, DNSMOS BAK/OVR, peor segundo p5).
3. Calidad y conservación de la voz (STOI, PESQ, DNSMOS SIG).
"""

import json
import os
import sys
import time
from pathlib import Path

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F
from einops import rearrange
from huggingface_hub import hf_hub_download
from safetensors.torch import load_file

# Importar métricas del harness del proyecto
sys.path.insert(0, str(Path(__file__).parent))
import metrics

# ─── Implementación Pure PyTorch CPU de Mamba S6 ──────────────────────────────

class PureMamba(nn.Module):
    def __init__(self, d_model=64, d_state=16, d_conv=4, expand=4, dt_rank=4):
        super().__init__()
        self.d_model = d_model
        self.d_state = d_state
        self.d_conv = d_conv
        self.expand = expand
        self.d_inner = d_model * expand
        self.dt_rank = dt_rank

        self.in_proj = nn.Linear(self.d_model, self.d_inner * 2, bias=False)
        self.conv1d = nn.Conv1d(
            self.d_inner,
            self.d_inner,
            kernel_size=d_conv,
            bias=True,
            groups=self.d_inner,
            padding=d_conv - 1,
        )
        self.x_proj = nn.Linear(self.d_inner, self.dt_rank + self.d_state * 2, bias=False)
        self.dt_proj = nn.Linear(self.dt_rank, self.d_inner, bias=True)
        self.A_log = nn.Parameter(torch.randn(self.d_inner, self.d_state))
        self.D = nn.Parameter(torch.ones(self.d_inner))
        self.out_proj = nn.Linear(self.d_inner, self.d_model, bias=False)

    def forward(self, u):
        B, L, D = u.shape
        xz = self.in_proj(u)
        x, z = xz.chunk(2, dim=-1)

        x_conv = self.conv1d(x.transpose(1, 2))[:, :, :L].transpose(1, 2)
        x_conv = F.silu(x_conv)

        x_dbl = self.x_proj(x_conv)
        dt, B_ssm, C_ssm = torch.split(x_dbl, [self.dt_rank, self.d_state, self.d_state], dim=-1)
        dt = F.softplus(self.dt_proj(dt))

        A = -torch.exp(self.A_log.float())
        y = torch.zeros(B, L, self.d_inner, device=u.device, dtype=u.dtype)
        h = torch.zeros(B, self.d_inner, self.d_state, device=u.device, dtype=torch.float32)

        for t in range(L):
            dt_t = dt[:, t, :]
            x_t = x_conv[:, t, :]
            B_t = B_ssm[:, t, :]
            C_t = C_ssm[:, t, :]

            dA = torch.exp(dt_t.unsqueeze(-1) * A.unsqueeze(0))
            dB = dt_t.unsqueeze(-1) * B_t.unsqueeze(1)
            h = dA * h + dB * x_t.unsqueeze(-1)
            y[:, t, :] = (h * C_t.unsqueeze(1)).sum(dim=-1) + self.D * x_t

        y = y * F.silu(z)
        return self.out_proj(y)


class MambaBlock(nn.Module):
    def __init__(self, d_model, cfg):
        super().__init__()
        self.forward_blocks = PureMamba(
            d_model=d_model,
            d_state=cfg["model_cfg"]["d_state"],
            d_conv=cfg["model_cfg"]["d_conv"],
            expand=cfg["model_cfg"]["expand"],
        )
        self.backward_blocks = PureMamba(
            d_model=d_model,
            d_state=cfg["model_cfg"]["d_state"],
            d_conv=cfg["model_cfg"]["d_conv"],
            expand=cfg["model_cfg"]["expand"],
        )
        self.output_proj = nn.Linear(2 * d_model, d_model)
        self.norm = nn.LayerNorm(d_model)

    def forward(self, x):
        out_fw = self.forward_blocks(x) + x
        out_bw = self.backward_blocks(torch.flip(x, dims=[1])) + torch.flip(x, dims=[1])
        out_bw = torch.flip(out_bw, dims=[1])
        out = torch.cat([out_fw, out_bw], dim=-1)
        out = self.output_proj(out)
        return self.norm(out)


class TFMambaBlock(nn.Module):
    def __init__(self, cfg):
        super().__init__()
        self.cfg = cfg
        self.hid_feature = cfg["model_cfg"]["hid_feature"]
        self.time_mamba = MambaBlock(d_model=self.hid_feature, cfg=cfg)
        self.freq_mamba = MambaBlock(d_model=self.hid_feature, cfg=cfg)

    def forward(self, x):
        b, c, t, f = x.size()
        x = x.permute(0, 3, 2, 1).contiguous().view(b * f, t, c)
        x = self.time_mamba(x) + x
        x = x.view(b, f, t, c).permute(0, 2, 1, 3).contiguous().view(b * t, f, c)
        x = self.freq_mamba(x) + x
        x = x.view(b, t, f, c).permute(0, 3, 1, 2)
        return x


def get_padding_2d(kernel_size, dilation=(1, 1)):
    return (
        int((kernel_size[0] * dilation[0] - dilation[0]) / 2),
        int((kernel_size[1] * dilation[1] - dilation[1]) / 2),
    )


class SPConvTranspose2d(nn.Module):
    def __init__(self, in_channels, out_channels, kernel_size, r=1):
        super().__init__()
        self.pad1 = nn.ConstantPad2d((1, 1, 0, 0), value=0.0)
        self.out_channels = out_channels
        self.conv = nn.Conv2d(in_channels, out_channels * r, kernel_size=kernel_size, stride=(1, 1))
        self.r = r

    def forward(self, x):
        x = self.pad1(x)
        out = self.conv(x)
        b, nchannels, H, W = out.shape
        out = out.view((b, self.r, nchannels // self.r, H, W))
        out = out.permute(0, 2, 3, 4, 1)
        out = out.contiguous().view((b, nchannels // self.r, H, -1))
        return out


class DenseBlock(nn.Module):
    def __init__(self, cfg, kernel_size=(3, 3), depth=4):
        super().__init__()
        self.dense_block = nn.ModuleList()
        hid = cfg["model_cfg"]["hid_feature"]
        for i in range(depth):
            dil = 2**i
            dense_conv = nn.Sequential(
                nn.Conv2d(
                    hid * (i + 1),
                    hid,
                    kernel_size,
                    dilation=(dil, 1),
                    padding=get_padding_2d(kernel_size, (dil, 1)),
                ),
                nn.InstanceNorm2d(hid, affine=True),
                nn.PReLU(hid),
            )
            self.dense_block.append(dense_conv)

    def forward(self, x):
        skip = x
        for block in self.dense_block:
            x = block(skip)
            skip = torch.cat([x, skip], dim=1)
        return x


class DenseEncoder(nn.Module):
    def __init__(self, cfg):
        super().__init__()
        in_ch = cfg["model_cfg"]["input_channel"]
        hid = cfg["model_cfg"]["hid_feature"]
        self.dense_conv_1 = nn.Sequential(
            nn.Conv2d(in_ch, hid, (1, 1)),
            nn.InstanceNorm2d(hid, affine=True),
            nn.PReLU(hid),
        )
        self.dense_block = DenseBlock(cfg, depth=4)
        self.dense_conv_2 = nn.Sequential(
            nn.Conv2d(hid, hid, (1, 3), stride=(4, 2)),
            nn.InstanceNorm2d(hid, affine=True),
            nn.PReLU(hid),
        )

    def forward(self, x):
        x = self.dense_conv_1(x)
        x = self.dense_block(x)
        x = self.dense_conv_2(x)
        return x


class MagDecoder(nn.Module):
    def __init__(self, cfg):
        super().__init__()
        self.dense_block = DenseBlock(cfg, depth=4)
        hid = cfg["model_cfg"]["hid_feature"]
        out_ch = cfg["model_cfg"]["output_channel"]
        self.up_conv1 = nn.Sequential(
            SPConvTranspose2d(hid, hid, (1, 3), 2),
            nn.InstanceNorm2d(hid, affine=True),
            nn.PReLU(hid),
        )
        self.up_conv2 = nn.Sequential(
            SPConvTranspose2d(hid, hid, (1, 3), 4),
            nn.InstanceNorm2d(hid, affine=True),
            nn.PReLU(hid),
        )
        self.final_conv = nn.Conv2d(hid, out_ch, (1, 1))

    def forward(self, x):
        x = self.dense_block(x)
        x = self.up_conv1(x)
        x = self.up_conv2(x.permute(0, 1, 3, 2)).permute(0, 1, 3, 2)
        x = self.final_conv(x)
        return x


class PhaseDecoder(nn.Module):
    def __init__(self, cfg):
        super().__init__()
        self.dense_block = DenseBlock(cfg, depth=4)
        hid = cfg["model_cfg"]["hid_feature"]
        out_ch = cfg["model_cfg"]["output_channel"]
        self.up_conv1 = nn.Sequential(
            SPConvTranspose2d(hid, hid, (1, 3), 2),
            nn.InstanceNorm2d(hid, affine=True),
            nn.PReLU(hid),
        )
        self.up_conv2 = nn.Sequential(
            SPConvTranspose2d(hid, hid, (1, 3), 4),
            nn.InstanceNorm2d(hid, affine=True),
            nn.PReLU(hid),
        )
        self.phase_conv_r = nn.Conv2d(hid, out_ch, (1, 1))
        self.phase_conv_i = nn.Conv2d(hid, out_ch, (1, 1))

    def forward(self, x):
        x = self.dense_block(x)
        x = self.up_conv1(x)
        x = self.up_conv2(x.permute(0, 1, 3, 2)).permute(0, 1, 3, 2)
        xr = self.phase_conv_r(x)
        xi = self.phase_conv_i(x)
        return torch.atan2(xi, xr)


class SEMambaModel(nn.Module):
    def __init__(self, cfg):
        super().__init__()
        self.cfg = cfg
        num_blocks = cfg["model_cfg"]["num_tfmamba"]
        self.dense_encoder = DenseEncoder(cfg)
        self.TSMamba = nn.ModuleList([TFMambaBlock(cfg) for _ in range(num_blocks)])
        self.mask_decoder = MagDecoder(cfg)
        self.phase_decoder = PhaseDecoder(cfg)

    def forward(self, noisy_mag, noisy_pha):
        noisy_mag = rearrange(noisy_mag, "b f t -> b t f").unsqueeze(1)
        noisy_pha = rearrange(noisy_pha, "b f t -> b t f").unsqueeze(1)
        x = torch.cat((noisy_mag, noisy_pha), dim=1)

        B, C, T, F = x.shape
        zeros = torch.zeros(B, C, T, 2, device=x.device)
        x = torch.cat((x, zeros), dim=-1)
        zeros = torch.zeros(B, C, 2, F + 2, device=x.device)
        x = torch.cat((x, zeros), dim=-2)

        x = self.dense_encoder(x)
        for block in self.TSMamba:
            x = block(x)

        denoised_mag = rearrange(self.mask_decoder(x), "b c t f -> b f t c").squeeze(-1)
        denoised_pha = rearrange(self.phase_decoder(x), "b c t f -> b f t c").squeeze(-1)
        denoised_mag = denoised_mag[:, :F, :T]
        denoised_pha = denoised_pha[:, :F, :T]
        return denoised_mag, denoised_pha


def make_even(value):
    val = int(round(value))
    return val if val % 2 == 0 else val + 1


def pad_or_trim_to_match(reference, target, pad_value=0.0):
    ref_len = reference.shape[-1]
    tgt_len = target.shape[-1]
    if tgt_len < ref_len:
        padding = (0, ref_len - tgt_len)
        return F.pad(target, padding, "constant", pad_value)
    elif tgt_len > ref_len:
        return target[..., :ref_len]
    return target


def enhance_audio(model, cfg, audio_tensor, sr):
    n_fft = cfg["stft_cfg"]["n_fft"]
    hop_size = cfg["stft_cfg"]["hop_size"]
    win_size = cfg["stft_cfg"]["win_size"]
    sampling_rate = cfg["stft_cfg"]["sampling_rate"]

    n_fft_scaled = make_even(n_fft * sr // sampling_rate)
    hop_size_scaled = make_even(hop_size * sr // sampling_rate)
    win_size_scaled = make_even(win_size * sr // sampling_rate)

    hann_win = torch.hann_window(win_size_scaled)
    stft_spec = torch.stft(
        audio_tensor,
        n_fft_scaled,
        hop_length=hop_size_scaled,
        win_length=win_size_scaled,
        window=hann_win,
        center=True,
        pad_mode="reflect",
        normalized=False,
        return_complex=True,
    )

    mag = torch.log1p(torch.abs(stft_spec))
    pha = torch.angle(stft_spec)

    with torch.no_grad():
        amp_g, pha_g = model(mag, pha)
        relu = nn.ReLU()
        mag_decompressed = torch.expm1(relu(amp_g))
        zero_portion = torch.sum(mag_decompressed == 0, 1) / mag_decompressed.shape[1]
        amp_g[:, :, (zero_portion > 0.5)[0]] = 0

        com = torch.complex(torch.expm1(relu(amp_g)) * torch.cos(pha_g), torch.expm1(relu(amp_g)) * torch.sin(pha_g))
        audio_out = torch.istft(
            com,
            n_fft_scaled,
            hop_length=hop_size_scaled,
            win_length=win_size_scaled,
            window=hann_win,
            center=True,
        )
        audio_out = pad_or_trim_to_match(audio_tensor, audio_out, pad_value=1e-8)
    return audio_out


def run_benchmark():
    os.makedirs("out/bench", exist_ok=True)
    in_wav = "tests/data/voz_noisy.wav"
    clean_wav = "tests/data/voz_clean.wav"
    out_nemotron = "out/bench/nemotron_voz_3s.wav"
    out_dpdfnet = "out/bench/voz_3s_out.wav"

    print("=== Evaluando NVIDIA NemotronLabs RE-USE en CPU ===")
    cfg_path = hf_hub_download("nvidia/NVIDIA-NemotronLabs-RE-USE", "config.json")
    with open(cfg_path) as f:
        cfg = json.load(f)

    print("Cargando modelo SEMamba (9.6M params)...")
    model = SEMambaModel(cfg)
    model.eval()

    weights_path = hf_hub_download("nvidia/NVIDIA-NemotronLabs-RE-USE", "model.safetensors")
    state_dict = load_file(weights_path)
    model.load_state_dict(state_dict, strict=True)
    print("Modelo cargado exitosamente en CPU.")

    import scipy.io.wavfile as wavfile
    sr, raw_data = wavfile.read(in_wav)
    if raw_data.dtype == np.int16:
        wav_np = raw_data.astype(np.float32) / 32768.0
    else:
        wav_np = raw_data.astype(np.float32)
    if wav_np.ndim > 1:
        wav_np = wav_np[:, 0]
    wav = torch.from_numpy(wav_np).unsqueeze(0)
    audio_dur = wav.shape[-1] / sr
    print(f"Audio de entrada: {in_wav} ({audio_dur:.2f} s @ {sr} Hz)")

    # Medir inferencia CPU
    t0 = time.perf_counter()
    enhanced = enhance_audio(model, cfg, wav, sr)
    t1 = time.perf_counter()

    elapsed = t1 - t0
    rtf = elapsed / audio_dur
    print(f"Tiempo CPU Nemotron: {elapsed:.3f} s (RTF = {rtf:.2f}x; <1.0 es más rápido que tiempo real)")

    out_np = enhanced.squeeze(0).clamp(-1.0, 1.0).numpy()
    out_int16 = (out_np * 32767.0).astype(np.int16)
    wavfile.write(out_nemotron, sr, out_int16)
    print(f"Guardado audio procesado en: {out_nemotron}")

    # Evaluar métricas de audio
    print("\n=== Calculando Métricas Acústicas ===")
    from benchmark import read_wav
    sig_clean, sr_clean = read_wav(Path(clean_wav))
    sig_noisy, _ = read_wav(Path(in_wav))
    sig_nemotron, _ = read_wav(Path(out_nemotron))

    in_sisdr = metrics.si_sdr(sig_clean, sig_noisy)
    nem_sisdr = metrics.si_sdr(sig_clean, sig_nemotron)
    nem_gain = nem_sisdr - in_sisdr
    nem_stoi = metrics.stoi_score(sig_clean, sig_nemotron, sr)
    nem_pesq = metrics.pesq_score(sig_clean, sig_nemotron, sr)
    nem_p5 = metrics.si_sdr_p5(sig_clean, sig_nemotron, sr)

    # DNSMOS
    nem_dnsmos = metrics.dnsmos_scores(sig_nemotron, sr)

    # Métricas DPDFNet de referencia
    if os.path.exists(out_dpdfnet):
        sig_dpdfnet, _ = read_wav(Path(out_dpdfnet))
        dp_sisdr = metrics.si_sdr(sig_clean, sig_dpdfnet)
        dp_gain = dp_sisdr - in_sisdr
        dp_stoi = metrics.stoi_score(sig_clean, sig_dpdfnet, sr)
        dp_pesq = metrics.pesq_score(sig_clean, sig_dpdfnet, sr)
        dp_p5 = metrics.si_sdr_p5(sig_clean, sig_dpdfnet, sr)
        dp_dnsmos = metrics.dnsmos_scores(sig_dpdfnet, sr)
    else:
        dp_sisdr = dp_gain = dp_stoi = dp_pesq = dp_p5 = None
        dp_dnsmos = {"sig": None, "bak": None, "ovr": None}

    dp_sig_str = f"{dp_dnsmos['sig']:.2f}" if dp_dnsmos['sig'] is not None else "-"
    dp_bak_str = f"{dp_dnsmos['bak']:.2f}" if dp_dnsmos['bak'] is not None else "-"
    dp_ovr_str = f"{dp_dnsmos['ovr']:.2f}" if dp_dnsmos['ovr'] is not None else "-"
    nem_sig_str = f"{nem_dnsmos['sig']:.2f}" if nem_dnsmos['sig'] is not None else "-"
    nem_bak_str = f"{nem_dnsmos['bak']:.2f}" if nem_dnsmos['bak'] is not None else "-"
    nem_ovr_str = f"{nem_dnsmos['ovr']:.2f}" if nem_dnsmos['ovr'] is not None else "-"

    print("\n" + "=" * 95)
    print(f"{'Métrica':<25} | {'Entrada (Noisy)':<18} | {'DPDFNet (Actual)':<20} | {'NemotronLabs RE-USE':<20}")
    print("-" * 95)
    print(f"{'Velocidad CPU (RTF)':<25} | {'-':<18} | {'~1.05 - 1.28x':<20} | {rtf:.2f}x")
    print(f"{'SI-SDR (dB)':<25} | {in_sisdr:+.2f} dB{'':<9} | {dp_sisdr:+.2f} dB{'':<11} | {nem_sisdr:+.2f} dB")
    print(f"{'Ganancia SI-SDR':<25} | {'-':<18} | {dp_gain:+.2f} dB{'':<11} | {nem_gain:+.2f} dB")
    print(f"{'Peor segundo (p5)':<25} | {'-':<18} | {dp_p5:+.2f} dB{'':<11} | {nem_p5:+.2f} dB")
    print(f"{'STOI (Inteligibilidad)':<25} | {'-':<18} | {dp_stoi:.3f}{'':<15} | {nem_stoi:.3f}")
    print(f"{'PESQ (Calidad Voz)':<25} | {'-':<18} | {dp_pesq:.3f}{'':<15} | {nem_pesq:.3f}")
    print(f"{'DNSMOS SIG (Voz)':<25} | {'-':<18} | {dp_sig_str:<20} | {nem_sig_str:<20}")
    print(f"{'DNSMOS BAK (Fondo)':<25} | {'-':<18} | {dp_bak_str:<20} | {nem_bak_str:<20}")
    print(f"{'DNSMOS OVR (Global)':<25} | {'-':<18} | {dp_ovr_str:<20} | {nem_ovr_str:<20}")
    print("=" * 95)


if __name__ == "__main__":
    run_benchmark()
