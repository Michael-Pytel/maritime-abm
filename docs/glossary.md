# Glossary

Accredited sub-model PDFs (when present) live in gitignored `citings/`. Full citations: [`report/mybib.bib`](../report/mybib.bib).

| Term | Meaning |
|---|---|
| **krABMaga** | Rust ABM framework (deterministic, data-parallel) |
| **CPA / TCPA** | Closest Point of Approach / Time to CPA — encounter geometry |
| **COLREGs** | International collision-avoidance regulations — give-way baseline |
| **DTU SIMCOL** | Probabilistic collision **damage/consequence** model (`simcol.rs`) |
| **IWRAP Mk II** | Regulator-grade collision-**frequency** model (`iwrap.rs`; also stamped per batch run) |
| **IAMSAR / MASSIM** | SAR manual + Koopman random-search detection (`sar.rs`) |
| **Ashrafi (2024)** | Month-conditioned SAR environmental degradation |
| **Xiao (2013)** | Optional artificial force-field steering (`forcefield.rs`) |
| **Karatas (2018)** | Man-overboard search framing |
| **`W`** | Local weather hazard ∈ [0,1] |
| **`P_prep`** | Crew preparedness = awareness × fatigue — scales comms success |
| **tick** | 5 simulated minutes; scenario horizons: Calm 14 d, Storm ≈4 d, Blind/Deep 7 d |
| **ship-hour** | 1/12 h per Active vessel per tick — KPI denominator |
| **`N_c`** | IWRAP expected collisions per year on the route network |
