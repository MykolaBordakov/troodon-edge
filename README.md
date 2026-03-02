# 🦖 Troodon Edge Proxy

![Troodon](https://img.shields.io/badge/Status-Active%20Development-success)
![Data Plane](https://img.shields.io/badge/Data%20Plane-Rust%20(Pingora)-orange)
![Control Plane](https://img.shields.io/badge/Control%20Plane-Go-blue)

**Troodon** — це високопродуктивний Edge Proxy, розроблений для забезпечення максимальної швидкості, безкомпромісної безпеки та гнучкості. Ми будуємо інфраструктуру майбутнього з динамічною конфігурацією та закладеною архітектурою під eBPF-плагіни для блискавичної фільтрації трафіку.

## 🌟 Чому Troodon? (The Engineering Excellence)
- **Blazing Fast Data Plane:** Написаний на **Rust** із використанням фреймворку [Pingora](https://github.com/cloudflare/pingora) від Cloudflare. Повний Zero-copy стрімінг та 100% асинхронність.
- **Smart Control Plane:** Легковажна та надійна система управління написана на **Go**.
- **Rock-Solid L7 Security:** Вбудований захист від HTTP-атак (напр., Slowloris), запобігання OOM через суворе лімітування розміру заголовків та надійний Circuit Breaker на базі `pingora-limits`.
- **Cascading Timeouts & Connection Pooling:** Просунута ієрархія тайм-аутів. Можливість визначати індивідуальні профілі тайм-аутів для кожного маршруту (напр. вічні з'єднання для WebSockets та короткі для REST API). Розумне перевикористання з'єднань (Keep-Alive) з бекендами з коробки.
- **Fast & Safe TLS Termination:** Завдяки інтеграції `rustls`, сертифікати шаряться між потоками без блокувань (Arc) та без I/O звернень до диска на гарячому шляху.

## 📚 Документація

Ми не тільки пишемо топ-код, але й створюємо детальну документацію. Ознайомтеся з нашими гайдами:

- 🛡️ **[Захист та Балансування (L7 Security & Timeouts)](./troodon/CONFIG_REFERENCE.md)** — Як налаштувати захист від Script Kiddies, ліміти коннектів та ієрархічно керувати пулами тайм-аутів.
- 🔒 **[Налаштування TLS (HTTPS Termination)](./troodon/TLS_REFERENCE.md)** — Інструкції з увімкнення "зеленого замочка" для ваших клієнтів та проксіювання безпечного трафіку до бекендів.

## 🚀 Швидкий старт (Data Plane)

1. Перейдіть у робочу директорію Data Plane:
   ```bash
   cd troodon
   ```
2. Зберіть та запустіть оптимізований сервер:
   ```bash
   cargo run --release
   ```

## 🏗️ Поточний статус
* **Data Plane (Rust):** Впроваджено базовий HTTP-фільтр, Radix-роутинг, активні Health Checks у фонових сервісах, підтримку Prometheus метрик, L7 Security та Connection Pooling.
* **Control Plane (Go):** Фаза архітектурного проектування. 

---
*Built with ❤️ for ultimate performance by top-tier engineers.*
