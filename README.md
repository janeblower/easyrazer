# EasyRazer

Лёгкая замена Razer Synapse для Razer Huntsman V2 Analog (`1532:0266`) на Windows.
Точка нажатия каждой клавиши (1.5–3.6 мм) записывается в память клавиатуры,
после этого она работает без какого-либо запущенного софта.

**Неофициальный проект, не связан с Razer Inc.**

## Что умеет

- показывает текущую точку нажатия каждой клавиши;
- меняет её для выделенных клавиш (клик, Ctrl+клик, рамка мышью);
- перед первой записью за сессию сохраняет бэкап в `%APPDATA%\EasyRazer`.

Rapid Trigger, Snap Tap, Dual-Step, бинды и подсветка пока не поддерживаются.

## Synapse

Synapse держит клавиатуру в своём режиме и перезаписывает настройки.
Перед работой закройте его полностью, включая значок в трее.
Пока он запущен, EasyRazer ничего не пишет.

## Сборка

Нужны Rust (MSVC), Node.js и MSVC Build Tools.

```
cd app
npm install
npm run tauri dev      # запуск
npm run tauri build    # target/release/easyrazer.exe
```

Тесты: `cargo test`. Тест на реальной клавиатуре (Synapse закрыт):
`cargo test -p easyrazer -- --ignored`.

## Структура

- `crates/core` — протокол Razer, без зависимостей от ОС (`hidapi` за фичей `hid`);
- `crates/probe` — CLI для исследования протокола;
- `app` — приложение (Tauri 2 + Vue).
