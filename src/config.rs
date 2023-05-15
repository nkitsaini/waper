use async_condvar_fair::Condvar;
use parking_lot::Mutex;
use std::sync::Arc;

struct CondvarData<T>
where
    T: Send + Clone,
{
    data: Mutex<T>,
    condvar: Condvar,
}

pub struct ConfigReciever<T>
where
    T: Send + Clone,
{
    inner: Arc<CondvarData<T>>,
}

#[derive(Clone)]
pub struct ConfigSender<T>
where
    T: Send + Clone,
{
    inner: Arc<CondvarData<T>>,
}

impl<T: Send + Clone> ConfigSender<T> {
    pub fn send(&mut self, value: T) {
        let mut v = self.inner.data.lock();
        *v = value;
        self.inner.condvar.notify_one();
    }
}

impl<T: Send + Clone> ConfigReciever<T> {
    pub async fn get_next(&self) -> T {
        let guard = self.inner.data.lock();
        let got = self.inner.condvar.wait(guard).await;
        return got.clone();
    }
    pub fn get_current(&self) -> T {
        self.inner.data.lock().clone()
    }
}

pub fn create_config<T>(initial: T) -> (ConfigSender<T>, ConfigReciever<T>)
where
    T: Send + Clone,
{
    let condvar = Condvar::default();
    let data = Mutex::new(initial);
    let inner = Arc::new(CondvarData { data, condvar });
    let sender = ConfigSender {
        inner: inner.clone(),
    };
    let reciever = ConfigReciever {
        inner: inner.clone(),
    };
    return (sender, reciever);
}
