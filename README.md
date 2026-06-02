# НейроРаб - OpenVK AI Bot

> ⚠️ **ВАЖНОЕ ПРЕДУПРЕЖДЕНИЕ**: Этот код был полностью написан искусственным интеллектом (Claude AI). 
> **Автор не ручается за качество кода, его функциональность и работу в production окружении.** 
> Используйте на свой риск и обязательно проверяйте код перед развертыванием.

НейроРаб (NeuroSlave) - это ИИ-бот для социальной сети OpenVK, разработанный на Rust. Бот отвечает на упоминания (@НейроРаб) в комментариях, используя Claude AI для генерации ответов, синхронизируя с контекстом беседы, поиском информации и анализом веб-ссылок.

## 🚀 Возможности

- **Контекстные ответы**: Бот учитывает историю комментариев в потоке
- **AI-генерация**: Ответы генерируются Claude Haiku 4.5 через tokenator.cloud
- **Веб-поиск**: Поиск информации через DuckDuckGo API
- **Анализ ссылок**: Автоматический анализ HTML-контента из предоставленных ссылок (до 10MB)
- **Кэширование**: SQLite база для сохранения обработанных комментариев и кэша
- **Логирование**: Логирование в файл и консоль

## 📋 Требования

- Rust 1.70+
- OpenVK API token
- Claude API key (через tokenator.cloud)
- SQLite3

## 🛠️ Установка и配置

1. **Клонируйте репозиторий**:
```bash
cd /home/spawnrys/neuro-rab-openvk
```

2. **Скопируйте и отредактируйте конфигурацию**:
```bash
cp .env.example .env
```

3. **Заполните .env файл**:
```env
OPENVK_API_URL=http://your-openvk-instance.com
OPENVK_API_TOKEN=your_token_here
OPENVK_BOT_ID=1

CLAUDE_API_URL=https://api.tokenator.cloud/v1
CLAUDE_API_KEY=your_claude_key_here
CLAUDE_MODEL=claude-3-5-haiku-20241022

DUCKDUCKGO_API_URL=https://api.duckduckgo.com

DATABASE_PATH=./bot_cache.db
POLLING_INTERVAL_SECS=6

LOG_LEVEL=info
LOG_FILE_PATH=./logs/bot.log
LOG_CONSOLE=true

MAX_PAGE_SIZE_MB=10
REQUEST_TIMEOUT_SECS=30

BOT_MENTION_PREFIX=@НейроРаб
BOT_NAME=НейроРаб
CONTEXT_MEMORY_SIZE=10
```

## 🏃 Запуск

```bash
# Режим разработки
cargo run

# Production режим
cargo run --release

# Запуск с логированием
RUST_LOG=info cargo run --release
```

## 📁 Структура проекта

```
src/
├── main.rs              # Главный файл с polling loop
├── config.rs            # Конфигурация из переменных окружения
├── logger.rs            # Система логирования
├── context.rs           # Управление контекстом и детектор упоминаний
├── db/
│   └── mod.rs          # SQLite база данных
├── openvk/
│   ├── mod.rs          # OpenVK типы и структуры
│   └── client.rs       # OpenVK API клиент
├── ai/
│   ├── mod.rs          # AI типы
│   └── claude.rs       # Claude API интеграция
└── web/
    ├── mod.rs          # Web типы
    ├── scraper.rs      # HTML парсер и скрейпер
    └── search.rs       # DuckDuckGo интеграция
```

## 🔄 Работа бота

1. **Polling Loop** (каждые 6 секунд):
   - Получает последние посты со стены
   - Проверяет комментарии на упоминание бота
   
2. **Обработка упоминания**:
   - Добавляет комментарий в контекст потока
   - Проверяет, нужен ли веб-поиск (ключевые слова: проверить, найти, check, search)
   - Генерирует ответ через Claude AI
   - Анализирует ссылки (если присутствуют)
   
3. **Постинг ответа**:
   - Отправляет ответ как reply к исходному комментарию
   - Сохраняет в базе данных для избегания дубликатов

## 💾 База данных

Автоматически создаёт таблицы:
- `processed_comments` - обработанные комментарии
- `context_cache` - кэш контекста потоков
- `web_cache` - кэш загруженного контента со ссылок

## ⚙️ Технологический стек

- **Tokio** - асинхронный runtime
- **Reqwest** - HTTP клиент
- **Serde** - сериализация/десериализация
- **Rusqlite** - SQLite интеграция
- **Scraper** - HTML парсинг
- **Tracing** - логирование и трассировка
- **Regex** - работа с регулярными выражениями

## 🤝 Поддержка и разработка

Для расширения функциональности:

1. **Добавление новых команд** - модифицируйте `MentionDetector` в `context.rs`
2. **Изменение AI behavior** - редактируйте system prompts в `claude.rs`
3. **Оптимизация polling** - измените `polling_interval_secs` в `.env`

## 📝 Лицензия

Проект для использования на OpenVK.

## 🐛 Troubleshooting

**Бот не отвечает**:
- Проверьте, что OpenVK API доступен
- Убедитесь, что bot_id правильный
- Проверьте логи в файле

**Ошибки при анализе ссылок**:
- Увеличьте `MAX_PAGE_SIZE_MB` если нужно
- Проверьте `REQUEST_TIMEOUT_SECS`

**API ошибки Claude**:
- Проверьте валидность API ключа
- Убедитесь в правильности URL tokenator.cloud
