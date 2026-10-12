# Модели клавиатур

Протокол и прошивка — по моделям. Здесь то, что у них общее, и чем они отличаются.

| Модель | VID:PID | MCU | EasyRazer | Документы |
|---|---|---|---|---|
| Huntsman V2 Analog | `1532:0266` | STM32L4+ | поддерживается | [протокол](huntsman-v2-analog/protocol.md), [прошивка](huntsman-v2-analog/firmware.md) |
| Huntsman V3 Pro | `1532:02A6` | NXP, вероятно LPC55xx | нет, только разбор | [протокол](huntsman-v3-pro/protocol.md), [прошивка](huntsman-v3-pro/firmware.md) |

V3 Pro не проверялась на железе: всё о ней — из модуля Synapse 4 и прошивки
1.03.00. Модули Synapse для V2 Analog, Huntsman Mini Analog (`0282`), V3 Pro,
V3 Pro TKL (`02A7`) и V3 Pro Mini (`02B0`) собраны из одной ветки, таблица
команд у V2 и V3 Pro одна; TKL и Mini, вероятно, отличаются от V3 Pro только
раскладкой.

## Пакет управления

Feature report 91 байт: report ID `00` + 90 байт. Формат одинаковый у обеих
моделей (у V3 Pro — по прошивке).

```
0      status: запрос 00; ответ 02 ok, 01 busy, 03 fail, 04 timeout, 05 not supported
1      transaction id (устройство принимает любой; используем 1F)
2-3    remaining packets
4      protocol type (0)
5      data size — фактическая длина payload
6      command class
7      command id; get = set | 0x80
8-87   args[80]
88     crc = XOR байтов 2..87
89     reserved
```

Обмен: `SetFeature` → `GetFeature`. Номер интерфейса и тонкости обработки — в
протоколе модели.

## Чем отличаются

| | V2 Analog | V3 Pro |
|---|---|---|
| Порог в протоколе | 1 байт, `0..255` ↔ 1.5–3.6 мм | u16 BE, 1638 = 0.1 мм, до 4.0 мм |
| Актуация | в назначении клавиши, `02:12` / `02:92` | `02:19` / `02:99`, до 15 клавиш в пакете; `02:12` тоже работает, порог — старший байт u16 |
| Rapid Trigger | только на хосте, в driver mode | в прошивке: `02:1A` / `02:9A`, `02:20` |
| Snap Tap | только на хосте | в прошивке, одна пара клавиш: `02:21` |
| Клавиш в таблице порогов | 126 | 135 |
| Подсветка | `0F:02/03/04`, LED `05`, эффекты `00`–`09` | то же, есть ещё эффект `0A` |

## Где брать данные о модели без железа

- **Модуль Synapse 4**: `apps.razer.com/synapse/products/<PID десятичный>/mw/`.
  `index.html` → `main.*.js` — описание устройства (`productId`, `analogSpecs`,
  флаги возможностей); в нём же карта чанков webpack. Чанк `3159` — таблица
  команд, `rzDevice25` — сборка пакетов, `obmEngineKeyboard` — какие команды
  вызываются для модели. Справка по протоколу, код не копировать.
- **Firmware Updater** с сайта Razer — .NET-программа, распаковывается 7-Zip.
  В `DeviceUpdater.resources` — параметры (`PID`, `BLPID`, `DevFWVer`) и образ:
  строки `DevFWLine0..N` — записи Intel HEX. Образ, если он открытый, разбирается
  в Ghidra.
