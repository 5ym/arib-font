//! 失敗の扱い (std だけ)。失敗はそのまま文で返す。

pub type Error = Box<dyn std::error::Error>;
pub type Result<T> = std::result::Result<T, Error>;

/// 文で失敗を返す
macro_rules! bail {
    ($($t:tt)*) => {
        return Err(format!($($t)*).into())
    };
}
pub(crate) use bail;

/// 失敗に説明を足す (Option は無いとき、Result は失敗のとき)
pub trait Ctx<T> {
    fn ctx(self, msg: impl FnOnce() -> String) -> Result<T>;
}

impl<T> Ctx<T> for Option<T> {
    fn ctx(self, msg: impl FnOnce() -> String) -> Result<T> {
        self.ok_or_else(|| msg().into())
    }
}

impl<T, E: std::fmt::Display> Ctx<T> for std::result::Result<T, E> {
    fn ctx(self, msg: impl FnOnce() -> String) -> Result<T> {
        self.map_err(|e| format!("{}: {e}", msg()).into())
    }
}
