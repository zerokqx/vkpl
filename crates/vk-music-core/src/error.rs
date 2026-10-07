pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Сессия VK истекла или недействительна. Выполните vkpl login")]
    Unauthorized,
    #[error("VK не разрешает этому приложению доступ к музыке (код {0})")]
    AccessDenied(i64),
    #[error("VK требует подтверждения в браузере (код {code})")]
    Challenge { code: i64 },
    #[error("VK ограничил частоту запросов; повторите позже")]
    RateLimited,
    #[error("Ошибка VK API: код {code}")]
    Api { code: i64 },
    #[error("Ошибка сети: {0}")]
    Transport(String),
    #[error("Неожиданный формат ответа VK: {0}")]
    Protocol(String),
    #[error("Неверный параметр: {0}")]
    InvalidInput(String),
    #[error("Трек недоступен для воспроизведения в этой сессии")]
    Unavailable,
}
