# bulb — План развития

## Текущее состояние

**Код**: ~4700 строк Rust, 52 теста, 0 предупреждений
**Репозиторий**: git@github.com:sehaxe/bulb.git (2 коммита)
**Компрессия**: bzip3 (.pkg.tar.bz3) — основной формат, zstd — совместимость

## Бенчмарки (последний прогон)

| Операция | pacman | bulb | Ускорение |
|----------|--------|------|-----------|
| Install zstd (1.9MB pkg) | 2007ms | 27ms | **74x** |
| Install bz3 (2.0MB pkg) | 2132ms | 635ms | **3.4x** |
| Query (all packages) | 62ms | 1ms | **62x** |
| Query bash | 78ms | 1ms | **78x** |
| vercmp (1M comparisons) | — | 379ms | 2.8x (было 1078ms) |
| Content store dedup | — | 51% savings | — |

### Что уже сделано
- Phase 0: core абстракции (version, dependency, pkginfo, pacman.conf parser)
- Phase 1: pacman read совместимость (desc, sync DB, local DB, mtree, pkgfile)
- Phase 2: content store с BLAKE3 dedup, generation rollback, транзакции
- Phase 3: download pipeline, sync repos, dependency resolver, PGP stub
- Phase 4 (частично): bz3 parallel decompression, benchmarks framework
- Оптимизации: BorrowedVersion (zero-alloc vercmp), HashMap desc parser

---

## Что осталось

### Phase 4: Оставшееся

#### 4.1 Sandbox builds (bwrap + landlock)
**Причина**: Сборка пакетов должна быть изолирована от системы.
**Реализация**:
- Новый модуль `src/sandbox.rs`
- CLI: `bulb build-sandbox <dir>` — сборка через bubblewrap
- Параметры sandbox:
  - `--unshare-pid` — отдельный PID namespace
  - `--unshare-net` — без сети (опционально)
  - `--ro-bind / /` — корень read-only
  - `--bind src_dir /src` — исходники read-write
  - `--tmpfs /tmp` — временная FS
- Проверка доступности bwrap через `which bwrap`
- landlock: проверка `/proc/self/landlock`, документирование как optional

#### 4.2 AUR PKGBUILD парсер
**Причина**: Поддержка AUR — основной источник пакетов Arch.
**Реализация**:
- Новый модуль `src/aur.rs`
- Парсинг PKGBUILD (bash) → `PackageInfo`
- PKGBUILD — это bash, не TOML. Парсим через:
  - regex для извлечения переменных (`pkgname=()`, `pkgver=`, `depends=()`)
  - Или через вызов `bash -c` для eval (проще, но медленнее)
- CLI: `bulb aur build <aur-url>` — clone + build
- CLI: `bulb aur search <query>` — поиск через AUR RPC

### Phase 5: TUI

#### 5.1 Интерфейс (ratatui + nucleo)
**Причина**: Удобный интерфейс для управления пакетами.
**Реализация**:
- Новый бинарник `src/bin/bulb-tui.rs` или subcommand `bulb tui`
- Зависимости: `ratatui`, `crossterm`, `nucleo` (fuzzy search)
- Экраны:
  - **Пакеты**: список установленных, поиск, информация
  - **Поиск в sync DB**: fuzzy search по имени/описанию
  - **Обзор**: что будет установлено/удалено
  - **Лог**: вывод операций в реальном времени
- Клавиатура:
  - `/` — поиск
  - `j/k` — навигация
  - `Enter` — установка/удаление
  - `q` — выход

### Phase 6: Продвинутое

#### 6.1 bulbd daemon
**Причина**: Фоновый процесс для автосинхронизации и кэширования.
**Реализация**:
- Новый бинарник `src/bin/bulbd.rs`
- systemd unit файл: `bulbd.service`
- IPC через unix socket (`/run/bulb/buld.sock`)
- Функции:
  - Периодическая синхронизация sync DB (interval из pacman.conf)
  - Предзагрузка пакетов в кэш
  - WebSocket для TUI (live обновления)
- JSON-RPC протокол

#### 6.2 Delta updates
**Причина**: Экономия трафика при обновлении.
**Реализация**:
- Новый модуль `src/delta.rs`
- Алгоритм: bsdiff/bspatch (bin) или xdelta3
- Формат: `.delta` файлы рядом с пакетами на зеркале
- CLI: `bulb delta <old> <new>` — генерация дельты
- CLI: `bulb install --delta` — установка через дельту
- Определение: если дельта < 30% от размера полного пакета — использовать

---

## Приоритеты

1. **Sandbox builds** — критично для безопасности
2. **AUR support** — критично для usability
3. **TUI** — важно для UX
4. **bulbd daemon** — важно для автоматизации
5. **Delta updates** — опционально, Nice-to-have

## Следующие шаги

Начать с sandbox builds (4.1) — это 가장 простой и важный модуль.
