// E:\shardvault-rs\src\sync\hlc.rs
// Hybrid Logical Clock (HLC) berdasarkan Algoritma Kulkarni et al. (2014)

use serde::{Deserialize, Serialize};
use std::cmp;
use std::fmt;
use std::str::FromStr;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::ShardVaultError;

/// Timestamp HLC yang merekam waktu fisik (l) dan penghitung logis (c)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HlcTimestamp {
    pub l: u64, // Komponen waktu fisik (milidetik)
    pub c: u32, // Komponen penghitung logis
}

impl HlcTimestamp {
    pub fn new(l: u64, c: u32) -> Self {
        Self { l, c }
    }

    /// Serialisasi timestamp menjadi string format kanonikal: "<l>.<c>"
    pub fn to_canonical_string(&self) -> String {
        format!("{}.{}", self.l, self.c)
    }
}

impl Ord for HlcTimestamp {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        match self.l.cmp(&other.l) {
            cmp::Ordering::Equal => self.c.cmp(&other.c),
            ord => ord,
        }
    }
}

impl PartialOrd for HlcTimestamp {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for HlcTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.l, self.c)
    }
}

impl FromStr for HlcTimestamp {
    type Err = ShardVaultError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 2 {
            return Err(ShardVaultError::Sync(format!(
                "Format HLC timestamp tidak valid: '{}' (harus '<l>.<c>')",
                s
            )));
        }
        let l = parts[0]
            .parse::<u64>()
            .map_err(|e| ShardVaultError::Sync(format!("Komponen l tidak valid: {}", e)))?;
        let c = parts[1]
            .parse::<u32>()
            .map_err(|e| ShardVaultError::Sync(format!("Komponen c tidak valid: {}", e)))?;

        Ok(HlcTimestamp { l, c })
    }
}

/// State Generator Hybrid Logical Clock untuk satu Node
pub struct HybridLogicalClock {
    pub node_id: String,
    state: Mutex<(u64, u32)>, // (l, c)
}

impl HybridLogicalClock {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            state: Mutex::new((0, 0)),
        }
    }

    fn get_physical_time() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    /// Mutasi event lokal atau pengiriman pesan (local_or_send)
    pub fn now(&self) -> HlcTimestamp {
        let pt = Self::get_physical_time();
        let mut state = self.state.lock().unwrap();
        let (ref mut l, ref mut c) = *state;

        if pt > *l {
            *l = pt;
            *c = 0;
        } else {
            *c += 1;
        }

        HlcTimestamp { l: *l, c: *c }
    }

    /// Pembaruan HLC saat menerima pesan dengan remote timestamp (receive)
    pub fn update(&self, remote_ts: HlcTimestamp) -> HlcTimestamp {
        let pt = Self::get_physical_time();
        let mut state = self.state.lock().unwrap();
        let (ref mut l, ref mut c) = *state;

        let max_l = cmp::max(cmp::max(*l, remote_ts.l), pt);

        if max_l == *l && max_l == remote_ts.l {
            *c = cmp::max(*c, remote_ts.c) + 1;
        } else if max_l == *l {
            *c += 1;
        } else if max_l == remote_ts.l {
            *c = remote_ts.c + 1;
        } else {
            *l = pt;
            *c = 0;
        }

        *l = max_l;
        HlcTimestamp { l: *l, c: *c }
    }
}
