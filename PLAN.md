# bulb — План развития

## Текущее состояние

**Код**: ~5000 строк Rust, 54 теста, 0 предупреждений
**Репозиторий**: git@github.com:sehaxe/bulb.git (3 коммита)
**Компрессия**: bzip3 (.pkg.tar.bz3) — основной формат, zstd — совместимость

## Бенчмарки (последний прогон)

| Операция | pacman | bulb | Ускорение |
|----------|--------|------|-----------|
| Install zstd (1.9MB pkg) | 1856ms | 25ms | **74x** |
| Install bz3 (2.0MB pkg) | 2334ms | 605ms | **3.9x** |
| Query (all packages) | 58ms | 1ms | **58x** |
| Query bash | 74ms | 1ms | **74x** |
| vercmp (1M comparisons) | — | 344ms | 3.1x (было 1078ms) |
| Content store dedup | — | 51% savings | — |
| Sync DB extra.db (8MB) | — | 205ms | — |

### Что уже сделано
- Phase 0: core абстракции (version, dependency, pkginfo, pacman.conf parser)
- Phase 1: pacman read совместимость (desc, sync DB, local DB, mtree, pkgfile)
- Phase 2: content store с BLAKE3 dedup, generation rollback, транзакции
- Phase 3: download pipeline, sync repos, dependency resolver, PGP stub
- Phase 4 (частично): bz3 parallel decompression, benchmarks framework
- Оптимизации: BorrowedVersion (zero-alloc vercmp), HashMap desc parser
- Архитектура: parallel pipeline (InstallPlan), mmap для decompression
- Deferred sudo: InstallPlan работает в /tmp, sudo нужен только для apply

---

## Архитектура (ключевые решения)

### Pipeline: download → verify → stage → apply
```
packages → [parallel extract] → staging/ → [single sudo apply] → / 
```
- Все операции без root
- параллельное извлечение через rayon
- mmap для zstd decompression (zero-copy)
- sudo запрашивается ОДИН раз в конце

### Content Store
```
package.tar.{zst,bz3} → BLAKE3 hash → content/ab/cdef... → hardlink → /usr/bin/xxx
```
- BLAKE3 хэш для dedup
- 2-char prefix bucketing
- hardlink вместо копии → 51% экономия

### Generation System
```
gen1 → gen2 → gen3 (current)
              ↓ rollback
         gen2 (current again)
```
- SQLite WAL для concurrency
- Каждая операция = новая генерация
- rollback = switch_generation_files (diff-based)

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

#### 4.3 Sync DB → SQLite индекс
**Причина**: Парсинг tar каждый раз = 205ms для extra.db. SQLite = <1ms.
**Реализация**:
- Новый модуль `src/sync_index.rs`
- При `bulb sync`: импорт sync DB → SQLite таблица `repo_packages`
- При `bulb search`: SELECT из SQLite (мгновенно)
- При `bulb install pkg`: поиск в SQLite

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

1. **Sync DB → SQLite** — мгновенный поиск (легко)
2. **Sandbox builds** — критично для безопасности
3. **AUR support** — критично для usability
4. **TUI** — важно для UX
5. **bulbd daemon** — важно для автоматизации
6. **Delta updates** — опционально, Nice-to-have

## Следующие шаги

Начать с sync DB → SQLite индекс — самый простой и эффективный выигрыш.
