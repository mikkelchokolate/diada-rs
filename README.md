# diada-rs

Программа анализа плоских рычажных механизмов на Rust: кинематика, силовой расчёт, движение, планы скоростей и ускорений.

В интерфейсе — «ДиАДА». Открывает механизмы `.DI` и сохраняет отчёты `.DIF`. Для работы DOSBox не нужен.

Сборка проверена на Windows x64. Для сборки нужны Rust и инструменты MSVC.

Из корня репозитория:

```powershell
cargo run --locked --manifest-path rust/Cargo.toml -p diada-rs
cargo build --release --locked --manifest-path rust/Cargo.toml -p diada-rs
```

Исполняемый файл: `rust/target/release/diada-rs.exe`.

Проверка:

```powershell
cargo test --workspace --locked --manifest-path rust/Cargo.toml
```

Код распространяется по MIT. Условия для численного модуля из Bochs — в `rust/crates/diada-compat/BOCHS-NUMERIC-NOTICE.txt`.
