# Troodon: Налаштування TLS (HTTPS)

## 1. Архітектура

Troodon використовує **OpenSSL** (через Pingora) для TLS termination. Сертифікати завантажуються один раз при старті та кешуються в пам'яті через `Arc` — без I/O на гарячому шляху.

## 2. Увімкнення Downstream TLS (клієнт → Troodon)

```yaml
server:
  listen_addr: "0.0.0.0"
  listen_port: 6188     # HTTP
  tls_port: 6443        # HTTPS

tls:
  certificates:
    main_app:
      cert: "/etc/ssl/certs/troodon.crt"   # PEM Full Chain
      key: "/etc/ssl/private/troodon.key"  # PEM Private Key (без паролю)
```

### Формати файлів

- **`cert`**: файл `.crt`/`.pem` з блоком `-----BEGIN CERTIFICATE-----`. Рекомендується Full Chain (сертифікат домену + проміжний CA).
- **`key`**: файл `.key`/`.pem` з блоком `-----BEGIN PRIVATE KEY-----`. Пароль на ключі не підтримується.

### Перевірка

```bash
# -k потрібен для самопідписаного сертифіката (тест)
curl -v -k https://127.0.0.1:6443/api
```

Лог при успішному старті:
```
🔒 Troodon is binding to TLS (HTTPS): 0.0.0.0:6443
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

> **Примітка:** Параметр `upstream_tls` має пріоритет над автодетектом по порту. Якщо відсутній — Troodon вмикає TLS тільки при порту 443.

SNI передається бекенду автоматично на основі поля `host` маршруту.
