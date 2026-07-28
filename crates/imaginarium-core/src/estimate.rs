//! Cost estimator (approximate; not a billing API).

use serde::{Deserialize, Serialize};

use crate::models::{self, ModelId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    pub model: String,
    pub unit: String,
    pub units: f64,
    pub estimated_usd: f64,
    pub note: String,
}

pub fn estimate_image(model: ModelId, n: u32) -> CostEstimate {
    let info = models::get(model);
    let units = f64::from(n.max(1));
    CostEstimate {
        model: model.as_str().into(),
        unit: info.unit.into(),
        units,
        estimated_usd: info.approx_usd_unit * units,
        note: "approx output cost only; input image fees may apply".into(),
    }
}

pub fn estimate_video(model: ModelId, duration_s: u32) -> CostEstimate {
    let info = models::get(model);
    let units = f64::from(duration_s.max(1));
    // Rough bump for 720p/1080p — catalog stores base tier.
    let mult = match model {
        ModelId::Video15 => 1.0, // base already ~480p-ish list; UI can refine later
        _ => 1.0,
    };
    CostEstimate {
        model: model.as_str().into(),
        unit: info.unit.into(),
        units,
        estimated_usd: info.approx_usd_unit * units * mult,
        note: "approx per-second output; resolution and inputs affect real bill".into(),
    }
}
