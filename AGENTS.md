# AGENTS.md — instrucciones para agentes de IA en este repo

## Verificación obligatoria (DoD)

```bash
cargo build --release
cargo clippy --all-targets -- -D warnings   # 0 warnings
cargo test --release                          # verde canónico: 33 passed / 5 ignored
python tools/quality/benchmark.py run --set tests_data --work out/bench
```

Ningún cambio se considera terminado sin estos cuatro en verde.

## Convenciones de documentación (se verifican)

- Docs en español. Tras tocar encabezados: paridad exacta TOC ↔ encabezados,
  anclas GitHub válidas, fences balanceados.
- Nunca duplicar números: el README apunta, `docs/<fichero>` es fuente de verdad.
- `CHANGELOG.md [Unreleased]` registra todo cambio visible.
- No crear ficheros `.py` con nombres de módulos stdlib (`struct.py`, …).
- Artefactos de corridas van a `out/` (gitignored); bytecode y logs jamás se commitean.

## Puerta de calidad del DSP

- El proceso iterativo completo vive en `docs/benchmark.md` §7: leerlo antes
  de tocar `src/df/`. Significado de métricas en `docs/metrics.md`.
- **Regla de oro**: `tools/quality/baseline.json` nunca se mueve para que un
  resultado pase; solo con `--update-baseline` para ratificar una mejora
  deliberada y verificada.
- Un cambio en el DSP por iteración; si la puerta falla, diagnosticar con la
  métrica y el clip que indica el reporte antes de reintentar.

## Estilo de commits

`scope: descripción` en español (`feat`, `fix`, `docs`), cuerpo con qué/cómo/
verificación. Inspeccionar `git status`, `git diff` y `git log` antes de
commitear; pushear a `main` solo con aprobación explícita.
