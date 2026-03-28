use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Instant,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

#[derive(Debug, Default)]
struct Stats {
    total_ops: u64,
    simulate_ops: u64,
    irrigate_ops: u64,
    pest_ops: u64,
    yield_ops: u64,
}

type AppState = Arc<Mutex<Stats>>;

// --- /api/v1/agri/simulate ---
#[derive(Debug, Deserialize)]
struct SimulateRequest {
    crop: String,
    planting_date_doy: u32, // day of year
    current_doy: u32,
    temp_avg_c: f64,
    rainfall_mm: f64,
    soil_type: String,
}

#[derive(Debug, Serialize)]
struct SimulateResponse {
    request_id: String,
    crop: String,
    growth_stage: String,
    biomass_kg_ha: f64,
    lai: f64, // leaf area index
    water_stress_index: f64,
    days_to_maturity: i32,
}

async fn simulate_crop(
    State(state): State<AppState>,
    Json(req): Json<SimulateRequest>,
) -> Result<Json<SimulateResponse>, StatusCode> {
    {
        let mut s = state.lock().unwrap();
        s.total_ops += 1;
        s.simulate_ops += 1;
    }

    let days_grown = req.current_doy.saturating_sub(req.planting_date_doy) as f64;
    // simplified logistic biomass growth
    let k = 12000.0_f64; // max biomass kg/ha
    let r = 0.05_f64; // growth rate
    let t0 = 40.0_f64; // inflection point days
    let biomass = k / (1.0 + (-(r * (days_grown - t0))).exp());

    let lai = (biomass / k * 6.0).min(6.0);
    let water_stress = if req.rainfall_mm < 3.0 { 0.8 } else { 0.1 };

    let total_days: i32 = match req.crop.to_lowercase().as_str() {
        "wheat" => 120,
        "rice" => 150,
        "corn" | "maize" => 130,
        "soybean" => 110,
        _ => 120,
    };
    let days_to_maturity = (total_days - days_grown as i32).max(0);

    let growth_stage = match days_grown as u32 {
        0..=10 => "germination",
        11..=30 => "vegetative",
        31..=70 => "reproductive",
        71..=100 => "grain_fill",
        _ => "maturity",
    }
    .to_string();

    Ok(Json(SimulateResponse {
        request_id: Uuid::new_v4().to_string(),
        crop: req.crop,
        growth_stage,
        biomass_kg_ha: (biomass * 10.0).round() / 10.0,
        lai: (lai * 100.0).round() / 100.0,
        water_stress_index: water_stress,
        days_to_maturity,
    }))
}

// --- /api/v1/agri/irrigate ---
#[derive(Debug, Deserialize)]
struct IrrigateRequest {
    field_area_ha: f64,
    crop: String,
    soil_moisture_pct: f64,
    et0_mm_day: f64, // reference evapotranspiration
    kc: f64,         // crop coefficient
}

#[derive(Debug, Serialize)]
struct IrrigateResponse {
    request_id: String,
    field_area_ha: f64,
    etc_mm_day: f64,   // crop evapotranspiration
    deficit_mm: f64,
    irrigation_mm: f64,
    total_volume_m3: f64,
    schedule: String,
}

async fn irrigation_schedule(
    State(state): State<AppState>,
    Json(req): Json<IrrigateRequest>,
) -> Result<Json<IrrigateResponse>, StatusCode> {
    {
        let mut s = state.lock().unwrap();
        s.total_ops += 1;
        s.irrigate_ops += 1;
    }

    let etc = req.et0_mm_day * req.kc;
    let field_capacity = 40.0_f64;
    let deficit = (field_capacity - req.soil_moisture_pct).max(0.0);
    let irrigation_mm = if deficit > 5.0 { deficit * 1.1 } else { 0.0 };
    let total_volume = irrigation_mm * req.field_area_ha * 10.0; // mm*ha -> m3/10

    let schedule = if irrigation_mm > 0.0 {
        "irrigate_tonight"
    } else {
        "no_irrigation_needed"
    }
    .to_string();

    Ok(Json(IrrigateResponse {
        request_id: Uuid::new_v4().to_string(),
        field_area_ha: req.field_area_ha,
        etc_mm_day: (etc * 100.0).round() / 100.0,
        deficit_mm: (deficit * 10.0).round() / 10.0,
        irrigation_mm: (irrigation_mm * 10.0).round() / 10.0,
        total_volume_m3: (total_volume * 10.0).round() / 10.0,
        schedule,
    }))
}

// --- /api/v1/agri/pest-risk ---
#[derive(Debug, Deserialize)]
struct PestRiskRequest {
    crop: String,
    temp_avg_c: f64,
    humidity_pct: f64,
    rainfall_7d_mm: f64,
    growth_stage: String,
}

#[derive(Debug, Serialize)]
struct PestRiskResponse {
    request_id: String,
    crop: String,
    risk_level: String,
    risk_score: f64,
    primary_threats: Vec<String>,
    recommendations: Vec<String>,
}

async fn pest_risk(
    State(state): State<AppState>,
    Json(req): Json<PestRiskRequest>,
) -> Result<Json<PestRiskResponse>, StatusCode> {
    {
        let mut s = state.lock().unwrap();
        s.total_ops += 1;
        s.pest_ops += 1;
    }

    let mut score = 0.0_f64;
    if req.humidity_pct > 80.0 {
        score += 30.0;
    }
    if req.temp_avg_c > 25.0 && req.temp_avg_c < 35.0 {
        score += 25.0;
    }
    if req.rainfall_7d_mm > 50.0 {
        score += 20.0;
    }
    if req.growth_stage == "reproductive" || req.growth_stage == "grain_fill" {
        score += 15.0;
    }

    let risk_level = match score as u32 {
        0..=20 => "LOW",
        21..=50 => "MEDIUM",
        51..=75 => "HIGH",
        _ => "CRITICAL",
    }
    .to_string();

    let mut threats = Vec::new();
    if req.humidity_pct > 80.0 {
        threats.push("fungal_blight".to_string());
    }
    if req.temp_avg_c > 28.0 {
        threats.push("aphids".to_string());
    }
    if req.rainfall_7d_mm > 50.0 {
        threats.push("root_rot".to_string());
    }

    let recommendations = if score > 50.0 {
        vec![
            "apply_fungicide".to_string(),
            "increase_field_monitoring".to_string(),
        ]
    } else {
        vec!["routine_scouting".to_string()]
    };

    Ok(Json(PestRiskResponse {
        request_id: Uuid::new_v4().to_string(),
        crop: req.crop,
        risk_level,
        risk_score: score,
        primary_threats: threats,
        recommendations,
    }))
}

// --- /api/v1/agri/yield ---
#[derive(Debug, Deserialize)]
struct YieldRequest {
    crop: String,
    field_area_ha: f64,
    soil_quality_score: f64, // 0-100
    avg_temp_c: f64,
    total_rainfall_mm: f64,
    fertilizer_kg_ha: f64,
}

#[derive(Debug, Serialize)]
struct YieldResponse {
    request_id: String,
    crop: String,
    predicted_yield_t_ha: f64,
    total_yield_t: f64,
    confidence_pct: f64,
    limiting_factor: String,
}

async fn yield_predict(
    State(state): State<AppState>,
    Json(req): Json<YieldRequest>,
) -> Result<Json<YieldResponse>, StatusCode> {
    {
        let mut s = state.lock().unwrap();
        s.total_ops += 1;
        s.yield_ops += 1;
    }

    let base_yield: f64 = match req.crop.to_lowercase().as_str() {
        "wheat" => 5.5,
        "rice" => 6.0,
        "corn" | "maize" => 9.0,
        "soybean" => 3.0,
        _ => 5.0,
    };

    let soil_factor = req.soil_quality_score / 100.0;
    let temp_factor = if req.avg_temp_c >= 15.0 && req.avg_temp_c <= 28.0 {
        1.0
    } else {
        0.8
    };
    let rain_factor = if req.total_rainfall_mm >= 400.0 && req.total_rainfall_mm <= 800.0 {
        1.0
    } else {
        0.85
    };
    let fert_factor = (1.0 + (req.fertilizer_kg_ha / 200.0).min(0.3)).min(1.3);

    let predicted = base_yield * soil_factor * temp_factor * rain_factor * fert_factor;
    let total = predicted * req.field_area_ha;

    let limiting = if soil_factor < 0.7 {
        "soil_quality"
    } else if temp_factor < 1.0 {
        "temperature"
    } else if rain_factor < 1.0 {
        "rainfall"
    } else {
        "none"
    }
    .to_string();

    Ok(Json(YieldResponse {
        request_id: Uuid::new_v4().to_string(),
        crop: req.crop,
        predicted_yield_t_ha: (predicted * 100.0).round() / 100.0,
        total_yield_t: (total * 100.0).round() / 100.0,
        confidence_pct: 78.5,
        limiting_factor: limiting,
    }))
}

// --- /api/v1/agri/stats ---
#[derive(Debug, Serialize)]
struct StatsResponse {
    service: &'static str,
    version: &'static str,
    total_ops: u64,
    simulate_ops: u64,
    irrigate_ops: u64,
    pest_ops: u64,
    yield_ops: u64,
}

async fn get_stats(State(state): State<AppState>) -> Json<StatsResponse> {
    let s = state.lock().unwrap();
    Json(StatsResponse {
        service: "alice-agri-core",
        version: env!("CARGO_PKG_VERSION"),
        total_ops: s.total_ops,
        simulate_ops: s.simulate_ops,
        irrigate_ops: s.irrigate_ops,
        pest_ops: s.pest_ops,
        yield_ops: s.yield_ops,
    })
}

// --- /health ---
#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
    uptime_secs: u64,
    total_ops: u64,
}

async fn health(
    State(state): State<AppState>,
    axum::extract::Extension(start): axum::extract::Extension<Arc<Instant>>,
) -> Json<HealthResponse> {
    let s = state.lock().unwrap();
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        uptime_secs: start.elapsed().as_secs(),
        total_ops: s.total_ops,
    })
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let state: AppState = Arc::new(Mutex::new(Stats::default()));
    let start = Arc::new(Instant::now());

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/v1/agri/simulate", post(simulate_crop))
        .route("/api/v1/agri/irrigate", post(irrigation_schedule))
        .route("/api/v1/agri/pest-risk", post(pest_risk))
        .route("/api/v1/agri/yield", post(yield_predict))
        .route("/api/v1/agri/stats", get(get_stats))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .layer(axum::extract::Extension(start))
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8125);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("alice-agri-core listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
