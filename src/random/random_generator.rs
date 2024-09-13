use rand::Rng;
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use tokio::sync::RwLock;
use std::sync::Arc;

pub struct RandomGenerator {
    rng: RwLock<ChaCha8Rng>,
}

impl RandomGenerator {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            rng: RwLock::new(ChaCha8Rng::from_entropy()),
        })
    }

    pub async fn gen_range(&self, min: u64, max: u64) -> u64 {
        let mut rng = self.rng.write().await;
        rng.gen_range(min..max)
    }

    pub async fn gen_bool(&self) -> bool {
        let mut rng = self.rng.write().await;
        rng.gen_bool(0.5)
    }
}
