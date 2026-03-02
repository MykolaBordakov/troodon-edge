# Троодон: Конфігурація та Безпека (L7 / Timeouts)

Цей документ описує нові можливості `config.yaml`, пов'язані з безпекою від L7-атак, гнучким налаштуванням тайм-аутів для бекендів (Connection Pooling) та захистом ресурсів операційної системи.

## 1. Захист від Script Kiddie (L7 Безпека)

Ці параметри застосовуються глобально до всього сервера (до маршрутизації) і мають на меті захистити **Troodon** від зловмисних клієнтів (таких як атаки Slowloris, гігантські HTTP заголовки, вичерпання файлових дескрипторів).

### Конфігурація `server:`
```yaml
server:
  # ... (інші параметри (IP, порту, логи))
  
  # Захист від Slowloris.
  # Максимальний час (в секундах) на читання HTTP-заголовків від клієнта.
  # Якщо клієнт не відправить заголовки вчасно, сервер закриє з'єднання.
  client_read_timeout: 10
  
  # Захист від вичерпання оперативної пам'яті (OOM).
  # Максимальний розмір всіх HTTP-заголовків клієнта разом узятих. 
  # Якщо ліміт перевищено, сервер поверне "HTTP 431 Request Header Fields Too Large"
  # і обірве з'єднання (Early Return) до перенаправлення на бекенд.
  max_header_size: 8192
  
  # Глобальний ліміт одночасних TCP/HTTP з'єднань клієнтів (Concurrency).
  # (Тільки логує параметри і вимагає відповідного `ulimit -n` в ОС)
  global_connections: 50000 
```

---

## 2. Повага до Бекендів (Upstream Management)

Troodon розроблений таким чином, щоб працювати в режимі **Zero-Copy Streaming** (пересилання байтів безпосередньо від клієнта до бекенду) і ефективно використовувати **Connection Pooling**.

Ми ввели **Каскадну (Ієрархічну) конфігурацію тайм-аутів**, яка дозволяє вказувати різні тайм-аути для різних типів трафіку (наприклад, 30 секунд для REST API, 1 година для WebSockets).

### 2.1 Глобальні (Дефолтні) `timeouts`

Ці тайм-аути розміщуються всередині блоку `server:` і є підстраховкою (fallback), якщо маршрут (Location) не вказав своїх власних.

```yaml
server:
  # Глобальні налаштування для з'єднань З БЕКЕНДАМИ (не клієнтами)
  timeouts:
    connect: 5 # Скільки часу дається на TCP Handshake з бекендом
    read: 10   # Тайм-аут читання першого байта від бекенда до проксі
    write: 10  # Тайм-аут запису на бекенд
    idle: 30   # [КРИТИЧНО] Скільки часу тримати відкритим "неактивне" tcp-з'єднання з бекендом 
               # в пулі (Keep-Alive) перед його розривом. Не ставити 0!
```

### 2.2 Маршрутні (Route-Specific) `timeouts` (Override)

Ви можете перевизначити глобальні тайм-аути безпосередньо у кожному `Location`. Це ідеально для спеціалізованих маршрутів, як-от WebSockets або довготривалих File Uploads.

```yaml
routes:
  - host: "api.example.com"
    locations:
      - path: "/ws-chat"
        upstreams: ["10.0.0.1:8080"]
        websocket: true
        # Ці тайм-аути "перекриють" (override) глобальні `server.timeouts`
        timeouts:
          connect: 2
          read: 3600  # Довге з'єднання (1 година) для веб-сокетів!
          write: 5
          idle: 3600  # Довге утримання TCP пулу для веб-сокетів
      
      # Якщо блок `timeouts` відсутній - візьмуться глобальні (fallbck)
      - path: "/api"
        upstreams: ["10.0.0.2:8080"]
```

### 2.3 Pingora Limits (Circuit Breakers)
Для захисту окремих бекендів можна вказати ліміт **Inflight** (максимальна кількість одночасних активних HTTP запитів):

```yaml
routes:
  - host: "api.example.com"
    locations:
      - path: "/heavy-pdf-generator"
        upstreams: ["10.0.0.3:8000"]
        max_inflight: 50 # Якщо активних запитів > 50, клієнту повернеться HTTP 503 Service Unavailable
```

---

## 3. Повний приклад конфігурації (`config.yaml`)

Ось приклад файлу з усією ієрархією:

```yaml
server:
  listen_addr: "0.0.0.0"
  listen_port: 6188
  log_level: "debug"
  
  # [NEW] L7 Security Defenses
  client_read_timeout: 10
  max_header_size: 8192
  global_connections: 50000
  
  # [NEW] Global Timeout Fallbacks
  timeouts:
    connect: 5
    read: 10
    write: 10
    idle: 30

routes:
  - host: "my-app.com"
    locations:
      # Звичайний REST API
      - path: "/api"
        upstreams: ["127.0.0.1:8000"]
        health_check_path: "/health"
        retry_count: 2
        # Тут застосовуються глобальні timeouts.

      # Важкий AI генератор з лімітами
      - path: "/ai-generate"
        upstreams: ["127.0.0.1:8001"]
        max_inflight: 5
        timeouts:
          connect: 5
          read: 120 # Генерація займає 2 хвилини
          write: 20
          idle: 10

      # Вебсокети
      - path: "/chat"
        upstreams: ["127.0.0.1:8002"]
        websocket: true
        timeouts:
          connect: 5
          read: 3600 # 1 година
          write: 10
          idle: 3600 # 1 година (не розриваємо пул швидко)
```
