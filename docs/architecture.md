# Архитектура


```
 Игрок A (хост)                                Игрок B
 ┌───────────────────────────┐                 ┌──────────────────────┐
 │ deadspace3.exe            │                 │ deadspace3.exe       │
 │  └ hosts:                 │                 │  └ hosts:            │
 │    gosredirector → 26.x.A │                 │    gosredirector →   │
 │ ds3-blaze (эмулятор)      │◄── Blaze/TCP ───┤    26.x.A            │
 │  :42127 редиректор+SSLv3  │    по VPN       │                      │
 │  :Blaze main, :QoS HTTP   │                 │                      │
 └────────────┬──────────────┘                 └──────────┬───────────┘
              └──────────── P2P UDP (ProtoTunnel) ────────┘
                         напрямую по Radmin VPN 26.x.x.x
```

- Эмулятор запускается у хоста (или на любом ПК в той же VPN). Внешний VPS не нужен.
- Для `NetworkAddress` каждого клиента эмулятор подставляет адрес, с которого пришло TCP-соединение (то есть VPN-адрес) и как внутренний, и как внешний. Так P2P гарантированно идёт через VPN, а demangler EA не нужен.
- Origin-токен эмулятор принимает любой и к EA за проверкой не обращается. Имя игрока задают ключи `--name` и `--player IP=NAME`.
- QoS эмулятор отдаёт сам (HTTP и UDP-эхо) в формате из захвата №2 ([captures.md](captures.md)), указывая свой адрес. Demangler EA не перенаправляется: адреса партнёров и так VPN-адреса, и в игре P2P устанавливается без него.
- Порт игры UDP 3659 должен быть открыт во входящих на VPN-адаптере у обоих игроков; это проверяет `p2p-check` ([network-checks.md](network-checks.md)).
- Гость попадает в комнату хоста «быстрой игрой» (публичная комната) или по приглашению из EA App (частная). Оба пути проверены в игре, подробности в разделе «Второй игрок» в [protocol.md](protocol.md). Списков игр эмулятор не ведёт.

## Существующие наработки

| Проект | Что даёт | Применимость к DS3 |
|---|---|---|
| [PocketRelay/Server](https://github.com/PocketRelay/Server) (Rust) | Полный Blaze-эмулятор ME3: редиректор, SSLv3, TDF, GameManager, матчмейкинг, QoS, работа в LAN/WAN, опциональная проверка Origin-токенов у EA | Источник раскладок сообщений и компонентов; сервер написан заново под DS3, заимствования указаны в заголовках исходников |
| [jacobtread/blaze-ssl-async](https://github.com/jacobtread/blaze-ssl-async) | SSLv3 (RC4-SHA/MD5) и сертификат-обход для старого ProtoSSL | Используется как зависимость; DS3 принимает сертификат-обход |
| [PocketRelay/Client](https://github.com/PocketRelay/Client) | Перенаправление клиента ME3 на свой сервер; в `/legacy` патч проверки сертификата из [masseffect-binkw32](https://github.com/Erik-JS/masseffect-binkw32) | Не понадобился: DS3 хватает правки `hosts` |
| [Aim4kill/BlazeSDK](https://github.com/Aim4kill/BlazeSDK) (C#) | Библиотека для Blaze-сервера: `Blaze2SDK`, `Blaze3SDK`, `EATDF`, `ProtoFire`, пример редиректора Blaze 3 | Источник определений команд и TDF Blaze 3; номера команд совпадают со строками exe DS3 |
| [PrivateServerEmulator/ME3PSE](https://github.com/PrivateServerEmulator/ME3PSE) (C#) | Старый эмулятор ME3 | Справочно |

Готового эмулятора для Dead Space 3 найти не удалось. Официальные серверы DS3, судя по всему, не закрыты (в отличие от DS2, отключённого 08.12.2023), но игроки массово жалуются на `Failed to join game`, а в обсуждениях советуют Hamachi и публичные сессии вместо приватных.

## Источники

- [PocketRelay](https://github.com/PocketRelay), [blaze-ssl-async](https://github.com/jacobtread/blaze-ssl-async), [Aim4kill/BlazeSDK](https://github.com/Aim4kill/BlazeSDK), [ME3PSE](https://github.com/PrivateServerEmulator/ME3PSE)
- [EA Forums: Dead Space 3 (PC) Failed To Join Game](https://forums.ea.com/discussions/dead-space-franchise-discussion-en/dead-space-3-pc-failed-to-join-game-error/7229623)
- [Steam: Does coop still work?](https://steamcommunity.com/app/1238060/discussions/0/634541622122515974/)
- [PC Gamer: EA closing servers incl. Dead Space 2](https://www.pcgamer.com/electronic-arts-is-closing-the-servers-for-even-more-old-games-including-dead-space-2-crysis-3-and-mirrors-edge-catalyst/)
