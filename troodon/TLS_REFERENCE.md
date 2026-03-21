# Troodon: Налаштування TLS (HTTPS)

## 1. Архітектура

Troodon використовує **OpenSSL** (через Pingora) для TLS termination. Підтримується **SNI-based multi-cert** — кожен домен отримує свій власний сертифікат на одному порту.

**Як це працює:**
1. При старті всі сертифікати завантажуються у пам'ять через `SslContextBuilder` (zero I/O на гарячому шляху)
2. Клієнт підключається й надсилає SNI (Extension в TLS ClientHello)
3. Troodon знаходить відповідний `SslContext` і повертає правильний сертифікат
4. **Немає SNI або невідомий домен → `TLS ALERT_FATAL`** — без дефолтних сертифікатів

---

## 2. Downstream TLS (клієнт → Troodon)

TLS конфігурується **на рівні route**, а не глобально. Кожен домен — свій сертифікат.

```yaml
# Глобальний HTTPS порт (один на всі домени)
tls_port: 6443

routes:
  # Домен 1 — свій сертифікат
  - host: "api.example.com"
    tls:
      cert: "/etc/ssl/certs/api.crt"    # PEM Full Chain
      key:  "/etc/ssl/private/api.key"  # PEM Private Key (без паролю)
      http2: true                       # Увімкнути HTTP/2 через ALPN
    locations:
      - path: "/api"
        upstreams: ["backend:8080"]

  # Домен 2 — інший сертифікат, той самий порт
  - host: "admin.example.com"
    tls:
      cert: "/etc/ssl/certs/admin.crt"
      key:  "/etc/ssl/private/admin.key"
    locations:
      - path: "/"
        upstreams: ["admin-backend:9000"]

  # Без TLS — тільки HTTP (через port 6188)
  - host: "internal.svc"
    # tls: відсутній = тільки HTTP
    locations:
      - path: "/"
        upstreams: ["internal:3000"]
```

### Формати файлів

- **`cert`**: `.crt`/`.pem` з `-----BEGIN CERTIFICATE-----`. Рекомендується Full Chain (домен + проміжний CA).
- **`key`**: `.key`/`.pem` з `-----BEGIN PRIVATE KEY-----`. Пароль на ключі **не підтримується**.
- **`client_ca`** (тільки при `mtls.enabled: true`): файл CA, який використовувався для підпису сертифікатів клієнтів.

### mTLS (Mutual TLS: перевірка клієнта)

Якщо вам потрібно автентифікувати клієнтів криптографічно, ви можете вказати блок `mtls` усередині конфігу `tls`.

```yaml
    tls:
      cert: "/etc/ssl/certs/admin.crt"
      key:  "/etc/ssl/private/admin.key"
      mtls:
        enabled: true
        client_ca: "/etc/ssl/certs/my_company_client_ca.crt"
```

Коли `mtls.enabled: true`:
- Сервер вимагатиме сертифікат від клієнта під час хендшейку (відбуватиметься повноцінна взаємна TLS-перевірка).
- Клієнтська сторона буде скинута (`TLS ALERT`), якщо сертифікат не був наданий або якщо він підписаний не тим `client_ca`, який налаштовано.
- **Fail Fast:** Якщо ви увімкнули mTLS, але файл `client_ca` відсутній на диску, Troodon **не запуститься**, щоб не допустити випадкового зниження безпеки.

### SNI — строга перевірка

| Ситуація | Результат |
|---|---|
| SNI є, домен налаштований | ✅ Отримує правильний сертифікат |
| SNI є, домен **не** налаштований | ❌ `TLS ALERT_FATAL` |
| SNI **відсутній** (старі клієнти) | ❌ `TLS ALERT_FATAL` |

> **Примітка:** Всі сучасні браузери та HTTP клієнти надсилають SNI. Відсутність SNI означає застарілий або некоректний клієнт.

### Перевірка

```bash
# Тест з конкретним SNI
curl -v -k --resolve api.example.com:6443:127.0.0.1 https://api.example.com:6443/api

# Самопідписаний сертифікат (dev)
curl -v -k https://127.0.0.1:6443/api
```

Лог при успішному старті:
```
🔒 TLS cert loaded for domain: api.example.com
🔒 TLS cert loaded for domain: admin.example.com
🔒 Troodon is binding to TLS (HTTPS): 0.0.0.0:6443 (2 domain(s))
```

---

## 3. Upstream TLS (Troodon → Backend)

З'єднання з бекендом через HTTPS налаштовується через параметр `upstream_tls` у блоці `locations`:

```yaml
routes:
  - host: "api.example.com"
    locations:
      # Явний TLS (будь-який порт)
      - path: "/secure"
        upstreams: ["internal-api:8443"]
        upstream_tls: true          # Явно увімкнути TLS

      # Автодетект: якщо порт == 443 і upstream_tls не вказано
      - path: "/external"
        upstreams: ["api.partner.com:443"]
        # upstream_tls не вказаний → автоматично true (порт 443)

      # Явно HTTP (щоб не плутатися з портом)
      - path: "/internal"
        upstreams: ["10.0.0.1:443"]
        upstream_tls: false         # Явно вимкнути TLS
```

> **Примітка:** `upstream_tls` має пріоритет над автодетектом. SNI для upstream передається з поля `host` маршруту (або `host_header`, якщо заданий).

---

## 4. Генерація тестового сертифіката

```bash
openssl req -x509 -newkey rsa:4096 -keyout test.key -out test.crt \
  -days 365 -nodes -subj "/CN=localhost"
```
