# ALICE-Agri-SaaS

Agricultural modeling API — part of the ALICE Eco-System.

## Overview

ALICE-Agri-SaaS provides precision agriculture computation: crop growth simulation, irrigation scheduling, pest risk assessment, and yield prediction using physics-based models.

## Services

- **core-engine** — Agronomic simulation, irrigation, pest, yield (port 8125)
- **api-gateway** — JWT auth, rate limiting, reverse proxy

## Quick Start

```bash
cd services/core-engine
cargo run

curl http://localhost:8125/health
```

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | /api/v1/agri/simulate | Crop growth simulation |
| POST | /api/v1/agri/irrigate | Irrigation scheduling |
| POST | /api/v1/agri/pest-risk | Pest risk assessment |
| POST | /api/v1/agri/yield | Yield prediction |
| GET  | /api/v1/agri/stats | Service statistics |

## License

AGPL-3.0-or-later
