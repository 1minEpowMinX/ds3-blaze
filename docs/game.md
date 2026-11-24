# Игра: что известно из exe

Дата анализа: 24.11.2026. Объект: `D:\Games\Steam\steamapps\common\Dead Space 3\deadspace3.exe` (Steam-версия, запуск через EA App, AppID 1238060).

## Протокол: да, это Blaze

Строки из секции `.rdata` (она не зашифрована):

| Признак | Значение |
|---|---|
| Версия SDK | `BlazeSDK 3.19.07.0`, сборка `Nov 27 2012` (путь `C:\ears\DL\006\packages\BlazeSDK\3.19.07.0\...`) |
| Имя сервиса для редиректора | `deadspace-3-pc` |
| Редиректор | `gosredirector.online.ea.com`, `gosredirector.ea.com` (`.scert`/`.stest` для тестовых окружений) |
| Авторизация | `Blaze::Authentication::OriginLoginRequest` (`originLogin`), токен берётся у EA App через Origin SDK (LSX, `127.0.0.1`) |
| Компоненты | `Authentication`, `Util` (`preAuth`, `postAuth`, `fetchClientConfig`, `userSettings*`, `getTelemetryServer`, `getTickerServer`), `UserSessions` (`updateNetworkInfo`, `lookupUser*`), `GameManager` (`createGame`, `joinGame`, `startMatchmaking`, `advanceGameState`, `updateMeshConnection`, `getGameListSnapshot`), `Locker`, `Playgroups`, статистика/лидерборды |
| QoS | HTTP-пинги `http://%s:%u/qos/qos`, `/qos/firetype`, `/qos/firewall` |

Набор команд и их имена совпадают с Mass Effect 3 (BlazeSDK 3.15.x, тот же 2012 год). Это значит, что кодек TDF и формат пакетов из эмуляторов ME3 должны подойти без переделки.

Проверено 24.11.2026: `gosredirector.ea.com` резолвится в `159.153.51.18`, порт `42127` открыт. Порт в самом бинарнике не найден, потому что код зашифрован (см. ниже), но 42127 стандартен для Blaze 3.

## SSL: тот же доверенный CA, что в ME3

Встроенная таблица доверенных корней DirtySock ProtoSSL (смещение файла около `0xB31300`):

- `OTG3 Certificate Authority` (Electronic Arts, Online Technology Group)
- `GOS 2011 Certificate Authority` (Global Online Studio)
- `VeriSign Class 3 Secure Server CA`

`OTG3` — тот самый корень, на который рассчитан сертификат-обход в `blaze-ssl-async` (Pocket Relay для ME3): «certificate built to bypass verification of older EA ProtoSSL implementations». DirtySock в DS3 того же поколения, поэтому велика вероятность, что обход сработает без патча клиента. Это первое, что нужно проверить экспериментом.

## Игровой трафик идёт P2P, Blaze только сводит игроков

В бинарнике есть весь сетевой стек DirtySock: `netgamelink`, `commudp-global`, `prototunnel-tunnel` (`EA Tunnel`), UPnP (`AddPortMapping`, флаг `-noupnp`), `demangler.ea.com` (ProtoMangle, NAT-пробивка через сервер EA), `peerport=`, а также перечисления топологий `PEER_TO_PEER_FULL_MESH`, `PEER_TO_PEER_PARTIAL_MESH`, `PEER_TO_PEER_DIRTYCAST_FAILOVER` и типов NAT `NAT_TYPE_OPEN/MODERATE/STRICT`.

Схема работы:

1. Клиент подключается к Blaze (через редиректор), логинится токеном Origin.
2. Хост создаёт игру (`createGame`), второй игрок находит её (матчмейкинг, список игр друзей или приглашение через EA App) и делает `joinGame`.
3. Blaze рассылает обоим `NotifyGameSetup` с сетевыми адресами друг друга (внутренний и внешний IP:порт).
4. Дальше игроки общаются напрямую по UDP через ProtoTunnel; при неудаче пытаются пробиться через demangler EA.

Именно шаги 3–4 ломаются у большинства: адреса берутся из QoS EA, а NAT-пробивка зависит от demangler. Если Blaze-сервер свой и выдаёт адреса из виртуальной сети (Radmin VPN, ZeroTier, Tailscale), проблема NAT исчезает целиком: пробивать нечего.

## Исполняемый файл зашифрован

Секция `.text` имеет энтропию 8.0, точка входа указывает на безымянную седьмую секцию (заглушка DRM EA, `Core\Activation.dll`). Для Ghidra нужен дамп процесса после расшифровки (x64dbg + Scylla, или дамп памяти работающей игры). Статически доступны только `.rdata`, `.data`, `.rsrc`. Отсюда же следствие для патчей: менять байты кода в файле нельзя, только в памяти после старта, через прокси-DLL (игра импортирует `VERSION.dll`, `DINPUT8.dll`, `WSOCK32.dll`, `d3dx9_43.dll`; `binkw32.dll`, как в ME3, здесь нет).
