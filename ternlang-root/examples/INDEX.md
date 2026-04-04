# Ternlang Examples

A growing collection of `.tern` programs demonstrating real-world decision logic in balanced ternary.

Every example follows the same pattern: three states (`-1` / `0` / `+1`) map to something concrete. The magic is always in the middle value — the state that binary systems are forced to throw away.

---

## Quick Reference

| State | Trit | Also called | Meaning |
|-------|------|-------------|---------|
| Reject | `-1` | `conflict()` | Clear negative signal. Do not proceed. |
| Hold | `0` | `hold()` | Not enough data. Wait or ask for more. |
| Affirm | `+1` | `truth()` | Clear positive signal. Proceed. |

---

## Examples

### Fundamentals
| # | File | Summary |
|---|------|---------|
| 01 | [01_hello_trit.tern](01_hello_trit.tern) | All three trit values, `invert()`, `consensus()` — start here |
| 02 | [02_decision_gate.tern](02_decision_gate.tern) | Safety as a hard gate: safety conflict blocks everything else |

### Real-World Decisions
| # | File | Summary |
|---|------|---------|
| 03 | [03_rocket_launch.tern](03_rocket_launch.tern) | Aerospace Go / No-Go / Hold; range safety as absolute veto |
| 04 | [04_sensor_fusion.tern](04_sensor_fusion.tern) | Autonomous vehicle four-sensor fusion; any obstacle signal dominates |
| 05 | [05_medical_triage.tern](05_medical_triage.tern) | ER triage; consciousness as hard gate |
| 06 | [06_git_merge.tern](06_git_merge.tern) | CI as hard gate; auto-merge / review / block |
| 07 | [07_spam_filter.tern](07_spam_filter.tern) | Email: Quarantine ≠ spam folder; hold is an active routing label |
| 08 | [08_evidence_collector.tern](08_evidence_collector.tern) | AI agents: low data density detection; formal "I need more" signal |

### Computer Science & Systems
| # | File | Summary |
|---|------|---------|
| 09 | [09_risc_fetch_decode.tern](09_risc_fetch_decode.tern) | CPU / Systems pipeline; stall = hold |
| 13 | [13_owlet_bridge.tern](13_owlet_bridge.tern) | Ternary S-expression eval loop; suspended eval = hold |
| 14 | [14_circuit_breaker.tern](14_circuit_breaker.tern) | Microservices: HALF-OPEN state is natively trit = 0 |
| 17 | [17_job_scheduler.tern](17_job_scheduler.tern) | Systems: Defer ≠ cancel; resource pressure produces hold |
| 19 | [19_cache_invalidation.tern](19_cache_invalidation.tern) | Web / CDN: Stale-while-revalidate is natively trit = 0 |

### Human Decisions & Civic Systems
| # | File | Summary |
|---|------|---------|
| 10 | [10_confidence_escalator.tern](10_confidence_escalator.tern) | AI agent self-assessment; escalate when uncertain |
| 11 | [11_form_validator.tern](11_form_validator.tern) | UX / Web: Empty ≠ invalid; ternary UX avoids hostile errors |
| 12 | [12_vote_aggregator.tern](12_vote_aggregator.tern) | Civic: Abstain is signal, not silence; quorum detection |
| 15 | [15_loan_underwriter.tern](15_loan_underwriter.tern) | Finance: Approve / refer to human / decline; automated humility |
| 16 | [16_content_moderation.tern](16_content_moderation.tern) | Trust & Safety: Allow / review / remove; human in the loop |
| 18 | [18_treaty_negotiation.tern](18_treaty_negotiation.tern) | Diplomacy: Veto ≠ reserve; failed ratification vs. procedural hold |
| 20 | [20_hiring_pipeline.tern](20_hiring_pipeline.tern) | HR: Hold bucket is the most valuable stage; references as soft gate |

### Engineering & Infrastructure
| # | File | Summary |
|---|------|---------|
| 21 | [21_nuclear_reactor.tern](21_nuclear_reactor.tern) | Nuclear reactor SCRAM / HOLD / NORMAL decision |
| 22 | [22_bridge_structural_health.tern](22_bridge_structural_health.tern) | Bridge structural health monitoring / warning / closure |
| 23 | [23_elevator_safety_interlock.tern](23_elevator_safety_interlock.tern) | Elevator safety interlock: floor alignment and door status |
| 24 | [24_chemical_plant_pressure.tern](24_chemical_plant_pressure.tern) | Chemical plant pressure relief valve control |
| 25 | [25_dam_water_level.tern](25_dam_water_level.tern) | Dam water level management: discharge / hold / fill |
| 26 | [26_power_grid_frequency.tern](26_power_grid_frequency.tern) | Power grid frequency stability monitoring |
| 27 | [27_wind_turbine_fatigue.tern](27_wind_turbine_fatigue.tern) | Wind turbine blade fatigue monitoring and maintenance |
| 28 | [28_oil_pipeline_leak.tern](28_oil_pipeline_leak.tern) | Oil pipeline leak detection and isolation |
| 29 | [29_aircraft_deicing.tern](29_aircraft_deicing.tern) | Aircraft deicing decision based on weather and queue |
| 30 | [30_runway_incursion.tern](30_runway_incursion.tern) | Runway incursion detection and ground control |
| 61 | [61_atc_conflict_alert.tern](61_atc_conflict_alert.tern) | Air traffic control conflict alert and resolution |
| 62 | [62_rail_block_occupancy.tern](62_rail_block_occupancy.tern) | Railway signal block occupancy and safety |
| 63 | [63_av_lane_change.tern](63_av_lane_change.tern) | Autonomous vehicle lane change safety assessment |
| 64 | [64_customs_clearance.tern](64_customs_clearance.tern) | Port of entry customs clearance and inspection |
| 65 | [65_drone_flight_authorization.tern](65_drone_flight_authorization.tern) | Drone flight authorization and airspace safety |
| 66 | [66_fleet_maintenance_dispatch.tern](66_fleet_maintenance_dispatch.tern) | Vehicle fleet maintenance scheduling and dispatch |
| 67 | [67_cold_chain_breach.tern](67_cold_chain_breach.tern) | Logistics cold chain temperature breach detection |
| 68 | [68_last_mile_delivery.tern](68_last_mile_delivery.tern) | Last-mile delivery attempt / reschedule / return |
| 69 | [69_adaptive_traffic_signal.tern](69_adaptive_traffic_signal.tern) | Adaptive traffic signal timing and congestion control |
| 70 | [70_ship_collision_avoidance.tern](70_ship_collision_avoidance.tern) | Maritime ship collision avoidance (COLREGs) |
| 101 | [101_solar_dispatch.tern](101_solar_dispatch.tern) | Solar power dispatch / curtail / storage decision |
| 102 | [102_battery_storage.tern](102_battery_storage.tern) | Battery energy storage charge/discharge management |
| 103 | [103_smart_meter_anomaly.tern](103_smart_meter_anomaly.tern) | Smart meter data anomaly and theft detection |
| 104 | [104_ev_charging.tern](104_ev_charging.tern) | EV charging session authorization and load balancing |
| 105 | [105_gas_regulator.tern](105_gas_regulator.tern) | Natural gas pressure regulator valve safety |
| 106 | [106_thermal_storage.tern](106_thermal_storage.tern) | Thermal energy storage dispatch and optimization |
| 107 | [107_renewable_curtailment.tern](107_renewable_curtailment.tern) | Renewable energy curtailment decision |
| 108 | [108_outage_isolation.tern](108_outage_isolation.tern) | Power grid outage isolation and restoration |
| 109 | [109_demand_response.tern](109_demand_response.tern) | Smart grid demand response event activation |
| 110 | [110_carbon_verification.tern](110_carbon_verification.tern) | Carbon offset verification and credit issuance |

### Medicine & Health
| # | File | Summary |
|---|------|---------|
| 31 | [31_drug_interaction.tern](31_drug_interaction.tern) | Drug interaction checker for safe prescribing |
| 32 | [32_icu_ventilator.tern](32_icu_ventilator.tern) | ICU ventilator weaning readiness assessment |
| 33 | [33_sepsis_warning.tern](33_sepsis_warning.tern) | Sepsis early warning system (SIRS/SOFA) |
| 34 | [34_radiology_flag.tern](34_radiology_flag.tern) | Radiology report urgent flag detection |
| 35 | [35_clinical_trial.tern](35_clinical_trial.tern) | Clinical trial eligibility and enrollment screening |
| 36 | [36_organ_transplant.tern](36_organ_transplant.tern) | Organ transplant compatibility and priority matching |
| 37 | [37_surgical_checklist.tern](37_surgical_checklist.tern) | Surgical go/no-go checklist for operating room safety |
| 38 | [38_antibiotic_resistance.tern](38_antibiotic_resistance.tern) | Antibiotic resistance risk and stewardship |
| 39 | [39_mental_health_triage.tern](39_mental_health_triage.tern) | Mental health crisis triage and intervention |
| 40 | [40_apgar_ternary.tern](40_apgar_ternary.tern) | Neonatal APGAR-inspired ternary assessment score |
| 119 | [119_quarantine_decision.tern](119_quarantine_decision.tern) | Public health quarantine / isolation / release decision |

### Finance & Risk
| # | File | Summary |
|---|------|---------|
| 41 | [41_insurance_claim.tern](41_insurance_claim.tern) | Automated insurance claim processing and fraud check |
| 42 | [42_trading_signal.tern](42_trading_signal.tern) | Volatility-aware market buy/sell/hold decision |
| 43 | [43_aml_transaction.tern](43_aml_transaction.tern) | Anti-Money Laundering (AML) transaction filtering |
| 44 | [44_options_expiry.tern](44_options_expiry.tern) | Options trading settlement and exercise decision |
| 45 | [45_portfolio_rebalance.tern](45_portfolio_rebalance.tern) | Wealth management portfolio drift control |
| 46 | [46_startup_due_diligence.tern](46_startup_due_diligence.tern) | Venture capital startup due diligence filter |
| 47 | [47_fraud_detection.tern](47_fraud_detection.tern) | E-commerce payment integrity and fraud detection |
| 48 | [48_central_bank_rate.tern](48_central_bank_rate.tern) | Central bank monetary policy interest rate decision |
| 49 | [49_crypto_withdrawal.tern](49_crypto_withdrawal.tern) | Digital asset custody withdrawal security gate |
| 50 | [50_invoice_authorization.tern](50_invoice_authorization.tern) | Accounts payable invoice authorization workflow |

### Legal & Governance
| # | File | Summary |
|---|------|---------|
| 51 | [51_bail_decision.tern](51_bail_decision.tern) | Pre-trial bail/release decision algorithm |
| 52 | [52_parole_review.tern](52_parole_review.tern) | Corrections parole eligibility and rehabilitation assessment |
| 53 | [53_patent_prior_art.tern](53_patent_prior_art.tern) | Patent prior art search and novelty examination |
| 54 | [54_contract_clause_risk.tern](54_contract_clause_risk.tern) | Legal document contract clause risk analysis |
| 55 | [55_immigration_visa.tern](55_immigration_visa.tern) | Border control and talent mobility visa assessment |
| 56 | [56_environmental_permit.tern](56_environmental_permit.tern) | Industrial environmental permit approval process |
| 57 | [57_building_code.tern](57_building_code.tern) | Building code compliance and safety inspection |
| 58 | [58_whistleblower_triage.tern](58_whistleblower_triage.tern) | Whistleblower report triage and investigation |
| 59 | [59_evidence_admissibility.tern](59_evidence_admissibility.tern) | Courtroom evidence admissibility and relevance gate |
| 60 | [60_regulatory_filing.tern](60_regulatory_filing.tern) | Corporate regulatory filing completeness and accuracy |

### Environment & Agriculture
| # | File | Summary |
|---|------|---------|
| 71 | [71_wildfire_risk.tern](71_wildfire_risk.tern) | Wildfire risk assessment based on fuel and weather |
| 72 | [72_flood_warning.tern](72_flood_warning.tern) | Flood warning and emergency evacuation trigger |
| 73 | [73_air_quality.tern](73_air_quality.tern) | Air quality index (AQI) monitoring and public advisory |
| 74 | [74_drought_irrigation.tern](74_drought_irrigation.tern) | Drought management and irrigation scheduling |
| 75 | [75_crop_disease.tern](75_crop_disease.tern) | Agricultural crop disease detection and treatment |
| 76 | [76_livestock_health.tern](76_livestock_health.tern) | Livestock health monitoring and disease outbreak gate |
| 77 | [77_harvest_timing.tern](77_harvest_timing.tern) | Optimal crop harvest timing based on maturity |
| 78 | [78_soil_contamination.tern](78_soil_contamination.tern) | Soil contamination classification and remediation |
| 79 | [79_aquaculture_oxygen.tern](79_aquaculture_oxygen.tern) | Aquaculture dissolved oxygen management |
| 80 | [80_pest_infestation.tern](80_pest_infestation.tern) | Pest infestation threshold and control decision |

### Security & Access Control
| # | File | Summary |
|---|------|---------|
| 81 | [81_multi_factor_auth.tern](81_multi_factor_auth.tern) | Multi-factor authentication (MFA) security gate |
| 82 | [82_biometric_liveness.tern](82_biometric_liveness.tern) | Biometric liveness and spoofing detection |
| 83 | [83_network_intrusion.tern](83_network_intrusion.tern) | Network intrusion detection system (NIDS) alerts |
| 84 | [84_physical_access.tern](84_physical_access.tern) | Physical building access control and tailgating |
| 85 | [85_privileged_access.tern](85_privileged_access.tern) | Privileged access management (PAM) authorization |
| 86 | [86_zero_trust_policy.tern](86_zero_trust_policy.tern) | Zero trust network access (ZTNA) policy enforcement |
| 87 | [87_firewall_rule.tern](87_firewall_rule.tern) | Firewall rule hit classification and packet filtering |
| 88 | [88_ransomware_detection.tern](88_ransomware_detection.tern) | Ransomware behavior detection and file protection |
| 89 | [89_supply_chain_integrity.tern](89_supply_chain_integrity.tern) | Software supply chain integrity and provenance check |
| 90 | [90_insider_threat.tern](90_insider_threat.tern) | Insider threat behavioral analysis and anomaly detection |

### Education & Research
| # | File | Summary |
|---|------|---------|
| 91 | [91_adaptive_test.tern](91_adaptive_test.tern) | Education: Adaptive test difficulty and progression gate |
| 92 | [92_student_at_risk.tern](92_student_at_risk.tern) | Student at-risk early warning and intervention |
| 93 | [93_scholarship_eligibility.tern](93_scholarship_eligibility.tern) | Scholarship eligibility scoring and award decision |
| 94 | [94_academic_integrity.tern](94_academic_integrity.tern) | Academic integrity and plagiarism detection gate |
| 95 | [95_research_ethics.tern](95_research_ethics.tern) | Research ethics board (IRB) approval workflow |
| 96 | [96_peer_review.tern](96_peer_review.tern) | Academic paper peer-review recommendation |
| 97 | [97_grant_completeness.tern](97_grant_completeness.tern) | Research grant application completeness check |
| 98 | [98_lab_safety.tern](98_lab_safety.tern) | Laboratory safety compliance and hazard check |
| 99 | [99_replication_crisis.tern](99_replication_crisis.tern) | Scientific replication study significance and validity |
| 100 | [100_phd_dissertation.tern](100_phd_dissertation.tern) | PhD dissertation defense / revision / fail decision |

### Social & Civic
| # | File | Summary |
|---|------|---------|
| 111 | [111_shelter_allocation.tern](111_shelter_allocation.tern) | Emergency shelter bed allocation and occupancy |
| 112 | [112_food_bank_eligibility.tern](112_food_bank_eligibility.tern) | Food bank eligibility and nutritional assistance |
| 113 | [113_refugee_status.tern](113_refugee_status.tern) | Refugee status determination and asylum processing |
| 114 | [114_cps_referral.tern](114_cps_referral.tern) | Child protective services (CPS) referral triage |
| 115 | [115_elder_care.tern](115_elder_care.tern) | Elder care assistance and facility placement |
| 116 | [116_disability_accommodation.tern](116_disability_accommodation.tern) | Workplace disability accommodation request review |
| 117 | [117_community_grant.tern](117_community_grant.tern) | Local community grant funding allocation |
| 118 | [118_noise_complaint.tern](118_noise_complaint.tern) | Municipal noise complaint escalation and enforcement |
| 120 | [120_housing_benefit.tern](120_housing_benefit.tern) | Social housing benefit eligibility and subsidy |

### Technology & Software
| # | File | Summary |
|---|------|---------|
| 121 | [121_api_rate_limit.tern](121_api_rate_limit.tern) | API rate limit enforcement and quota management |
| 122 | [122_database_query.tern](122_database_query.tern) | Database query classification and optimization |
| 123 | [123_deployment_readiness.tern](123_deployment_readiness.tern) | Software deployment readiness and smoke test gate |
| 124 | [124_ab_test_significance.tern](124_ab_test_significance.tern) | A/B test statistical significance and rollout |
| 125 | [125_bug_severity.tern](125_bug_severity.tern) | Software bug severity and priority triage |
| 126 | [126_code_review_gate.tern](126_code_review_gate.tern) | Code review approval and merge gate |
| 127 | [127_vulnerability_check.tern](127_vulnerability_check.tern) | Software vulnerability scan and patching decision |
| 128 | [128_container_health.tern](128_container_health.tern) | Container liveness and readiness probe logic |
| 129 | [129_feature_flag_rollout.tern](129_feature_flag_rollout.tern) | Feature flag percentage rollout and canary gate |
| 130 | [130_dns_resolution.tern](130_dns_resolution.tern) | DNS resolution confidence and failover decision |

### Sports & Entertainment
| # | File | Summary |
|---|------|---------|
| 131 | [131_referee_challenge.tern](131_referee_challenge.tern) | Sports referee challenge review and reversal |
| 132 | [132_athlete_injury_risk.tern](132_athlete_injury_risk.tern) | Athlete injury risk assessment before competition |
| 133 | [133_doping_test.tern](133_doping_test.tern) | Anti-doping test result gate and investigation |
| 134 | [134_film_rating.tern](134_film_rating.tern) | Film rating board content classification |
| 135 | [135_music_rights_clearance.tern](135_music_rights_clearance.tern) | Music rights and royalty clearance workflow |
| 136 | [136_streaming_quality.tern](136_streaming_quality.tern) | Adaptive bitrate streaming quality adjustment |
| 137 | [137_esports_anti_cheat.tern](137_esports_anti_cheat.tern) | Esports anti-cheat detection and ban logic |
| 138 | [138_track_condition.tern](138_track_condition.tern) | Racing track condition (fast/sloppy/muddy) flag |
| 139 | [139_broadcasting_rights.tern](139_broadcasting_rights.tern) | Sports broadcasting rights geo-fencing gate |
| 140 | [140_weather_gate.tern](140_weather_gate.tern) | Outdoor event weather safety go/no-go gate |

---

### Extended Domain Examples (141-250)
| # | File | Summary |
|---|------|---------|
| 141 | [141_space_mission_planning.tern](141_space_mission_planning.tern) | Space Mission Planning |
| 142 | [142_autonomous_ship_routing.tern](142_autonomous_ship_routing.tern) | Autonomous Ship Routing |
| 143 | [143_insurance_actuarial.tern](143_insurance_actuarial.tern) | Insurance Actuarial Tables |
| 144 | [144_pandemic_tracing.tern](144_pandemic_tracing.tern) | Pandemic Contact Tracing |
| 145 | [145_carbon_verification.tern](145_carbon_verification.tern) | Carbon Credit Verification |
| 146 | [146_water_treatment.tern](146_water_treatment.tern) | Water Treatment Plant |
| 147 | [147_warehouse_robot.tern](147_warehouse_robot.tern) | Autonomous Warehouse Robot |
| 148 | [148_clinical_genomics.tern](148_clinical_genomics.tern) | Clinical Genomics |
| 149 | [149_satellite_collision.tern](149_satellite_collision.tern) | Satellite Collision Avoidance |
| 150 | [150_recidivism_risk.tern](150_recidivism_risk.tern) | Criminal Recidivism Risk |
| 151 | [151_earthquake_warning.tern](151_earthquake_warning.tern) | Earthquake Early Warning |
| 152 | [152_autonomous_submarine.tern](152_autonomous_submarine.tern) | Autonomous Submarine |
| 153 | [153_supply_chain.tern](153_supply_chain.tern) | Supply Chain Disruption |
| 154 | [154_nuclear_sub_reactor.tern](154_nuclear_sub_reactor.tern) | Nuclear Submarine Reactor |
| 155 | [155_wildfire_evacuation.tern](155_wildfire_evacuation.tern) | Wildfire Evacuation Routing |
| 156 | [156_hospital_bed.tern](156_hospital_bed.tern) | Hospital Bed Allocation |
| 157 | [157_bridge_toll.tern](157_bridge_toll.tern) | Bridge Toll Pricing |
| 158 | [158_food_recall.tern](158_food_recall.tern) | Food Safety Recall |
| 159 | [159_vaccine_cold_chain.tern](159_vaccine_cold_chain.tern) | Vaccine Cold Chain |
| 160 | [160_deepfake_detection.tern](160_deepfake_detection.tern) | Deepfake Detection |
| 161 | [161_satellite_uplink.tern](161_satellite_uplink.tern) | Satellite Uplink Control |
| 162 | [162_social_media_suspension.tern](162_social_media_suspension.tern) | Social Media Account Suspension |
| 163 | [163_employee_review.tern](163_employee_review.tern) | Employee Performance Review |
| 164 | [164_tax_fraud.tern](164_tax_fraud.tern) | Tax Fraud Flagging |
| 165 | [165_drug_interdiction.tern](165_drug_interdiction.tern) | Customs Drug Interdiction |
| 166 | [166_piracy_alert.tern](166_piracy_alert.tern) | Maritime Piracy Alert |
| 167 | [167_avalanche_risk.tern](167_avalanche_risk.tern) | Avalanche Risk Assessment |
| 168 | [168_tornado_intercept.tern](168_tornado_intercept.tern) | Tornado Intercept Strategy |
| 169 | [169_black_hole_scheduling.tern](169_black_hole_scheduling.tern) | Black Hole Observation Scheduling |
| 170 | [170_cern_beam_abort.tern](170_cern_beam_abort.tern) | CERN Beam Abort |
| 171 | [171_quantum_processor_stability.tern](171_quantum_processor_stability.tern) | Quantum Processor Stability |
| 172 | [172_cyber_physical_system_integrity.tern](172_cyber_physical_system_integrity.tern) | Cyber-Physical System Integrity |
| 173 | [173_biodiversity_monitoring.tern](173_biodiversity_monitoring.tern) | Biodiversity Monitoring |
| 174 | [174_microgrid_fault_detection.tern](174_microgrid_fault_detection.tern) | Microgrid Fault Detection |
| 175 | [175_space_debris_tracking.tern](175_space_debris_tracking.tern) | Space Debris Tracking |
| 176 | [176_geothermal_energy_extraction.tern](176_geothermal_energy_extraction.tern) | Geothermal Energy Extraction |
| 177 | [177_precision_agriculture_spraying.tern](177_precision_agriculture_spraying.tern) | Precision Agriculture Spraying |
| 178 | [178_smart_city_traffic_flow.tern](178_smart_city_traffic_flow.tern) | Smart City Traffic Flow |
| 179 | [179_industrial_robot_safety.tern](179_industrial_robot_safety.tern) | Industrial Robot Safety |
| 180 | [180_waste_sorting_automation.tern](180_waste_sorting_automation.tern) | Waste Sorting Automation |
| 181 | [181_pipeline_corrosion_monitoring.tern](181_pipeline_corrosion_monitoring.tern) | Pipeline Corrosion Monitoring |
| 182 | [182_air_traffic_flow_management.tern](182_air_traffic_flow_management.tern) | Air Traffic Flow Management |
| 183 | [183_emergency_dispatch_prioritization.tern](183_emergency_dispatch_prioritization.tern) | Emergency Dispatch Prioritization |
| 184 | [184_fleet_fuel_optimization.tern](184_fleet_fuel_optimization.tern) | Fleet Fuel Optimization |
| 185 | [185_supply_chain_provenance.tern](185_supply_chain_provenance.tern) | Supply Chain Provenance |
| 186 | [186_water_desalination_control.tern](186_water_desalination_control.tern) | Water Desalination Control |
| 187 | [187_renewable_energy_integration.tern](187_renewable_energy_integration.tern) | Renewable Energy Integration |
| 188 | [188_energy_storage_optimization.tern](188_energy_storage_optimization.tern) | Energy Storage Optimization |
| 189 | [189_building_energy_management.tern](189_building_energy_management.tern) | Building Energy Management |
| 190 | [190_smart_lighting_control.tern](190_smart_lighting_control.tern) | Smart Lighting Control |
| 191 | [191_autonomous_lawn_mower.tern](191_autonomous_lawn_mower.tern) | Autonomous Lawn Mower |
| 192 | [192_robotic_surgery_guidance.tern](192_robotic_surgery_guidance.tern) | Robotic Surgery Guidance |
| 193 | [193_patient_vital_monitoring.tern](193_patient_vital_monitoring.tern) | Patient Vital Monitoring |
| 194 | [194_elderly_fall_detection.tern](194_elderly_fall_detection.tern) | Elderly Fall Detection |
| 195 | [195_drug_discovery_simulation.tern](195_drug_discovery_simulation.tern) | Drug Discovery Simulation |
| 196 | [196_clinical_trial_recruitment.tern](196_clinical_trial_recruitment.tern) | Clinical Trial Recruitment |
| 197 | [197_medical_imaging_analysis.tern](197_medical_imaging_analysis.tern) | Medical Imaging Analysis |
| 198 | [198_public_health_surveillance.tern](198_public_health_surveillance.tern) | Public Health Surveillance |
| 199 | [199_vaccine_distribution_logistics.tern](199_vaccine_distribution_logistics.tern) | Vaccine Distribution Logistics |
| 200 | [200_health_insurance_fraud.tern](200_health_insurance_fraud.tern) | Health Insurance Fraud |
| 201 | [201_financial_market_surveillance.tern](201_financial_market_surveillance.tern) | Financial Market Surveillance |
| 202 | [202_algorithmic_trading_risk.tern](202_algorithmic_trading_risk.tern) | Algorithmic Trading Risk |
| 203 | [203_credit_risk_assessment.tern](203_credit_risk_assessment.tern) | Credit Risk Assessment |
| 204 | [204_anti_money_laundering.tern](204_anti_money_laundering.tern) | Anti-Money Laundering |
| 205 | [205_insurance_policy_underwriting.tern](205_insurance_policy_underwriting.tern) | Insurance Policy Underwriting |
| 206 | [206_claim_processing_automation.tern](206_claim_processing_automation.tern) | Claim Processing Automation |
| 207 | [207_tax_compliance_monitoring.tern](207_tax_compliance_monitoring.tern) | Tax Compliance Monitoring |
| 208 | [208_government_budget_allocation.tern](208_government_budget_allocation.tern) | Government Budget Allocation |
| 209 | [209_public_procurement_integrity.tern](209_public_procurement_integrity.tern) | Public Procurement Integrity |
| 210 | [210_e_voting_security_audit.tern](210_e_voting_security_audit.tern) | E-voting Security Audit |
| 211 | [211_social_media_content_moderation.tern](211_social_media_content_moderation.tern) | Social Media Content Moderation |
| 212 | [212_online_harassment_detection.tern](212_online_harassment_detection.tern) | Online Harassment Detection |
| 213 | [213_recommendation_system_ethics.tern](213_recommendation_system_ethics.tern) | Recommendation System Ethics |
| 214 | [214_digital_identity_verification.tern](214_digital_identity_verification.tern) | Digital Identity Verification |
| 215 | [215_privacy_policy_compliance.tern](215_privacy_policy_compliance.tern) | Privacy Policy Compliance |
| 216 | [216_data_breach_detection.tern](216_data_breach_detection.tern) | Data Breach Detection |
| 217 | [217_cloud_resource_allocation.tern](217_cloud_resource_allocation.tern) | Cloud Resource Allocation |
| 218 | [218_edge_computing_orchestration.tern](218_edge_computing_orchestration.tern) | Edge Computing Orchestration |
| 219 | [219_5g_network_slicing.tern](219_5g_network_slicing.tern) | 5G Network Slicing |
| 220 | [220_iot_device_security.tern](220_iot_device_security.tern) | IoT Device Security |
| 221 | [221_satellite_communication_handover.tern](221_satellite_communication_handover.tern) | Satellite Communication Handover |
| 222 | [222_autonomous_underwater_vehicle.tern](222_autonomous_underwater_vehicle.tern) | Autonomous Underwater Vehicle |
| 223 | [223_lunar_rover_navigation.tern](223_lunar_rover_navigation.tern) | Lunar Rover Navigation |
| 224 | [224_mars_habitat_life_support.tern](224_mars_habitat_life_support.tern) | Mars Habitat Life Support |
| 225 | [225_asteroid_mining_feasibility.tern](225_asteroid_mining_feasibility.tern) | Asteroid Mining Feasibility |
| 226 | [226_space_weather_forecasting.tern](226_space_weather_forecasting.tern) | Space Weather Forecasting |
| 227 | [227_exoplanet_habitability_analysis.tern](227_exoplanet_habitability_analysis.tern) | Exoplanet Habitability Analysis |
| 228 | [228_seismic_data_interpretation.tern](228_seismic_data_interpretation.tern) | Seismic Data Interpretation |
| 229 | [229_volcano_eruption_prediction.tern](229_volcano_eruption_prediction.tern) | Volcano Eruption Prediction |
| 230 | [230_ocean_current_modeling.tern](230_ocean_current_modeling.tern) | Ocean Current Modeling |
| 231 | [231_glacier_retreat_monitoring.tern](231_glacier_retreat_monitoring.tern) | Glacier Retreat Monitoring |
| 232 | [232_deforestation_detection.tern](232_deforestation_detection.tern) | Deforestation Detection |
| 233 | [233_wildlife_poaching_prevention.tern](233_wildlife_poaching_prevention.tern) | Wildlife Poaching Prevention |
| 234 | [234_air_pollution_source_apportionment.tern](234_air_pollution_source_apportionment.tern) | Air Pollution Source Apportionment |
| 235 | [235_water_quality_monitoring.tern](235_water_quality_monitoring.tern) | Water Quality Monitoring |
| 236 | [236_soil_health_assessment.tern](236_soil_health_assessment.tern) | Soil Health Assessment |
| 237 | [237_urban_heat_island_mitigation.tern](237_urban_heat_island_mitigation.tern) | Urban Heat Island Mitigation |
| 238 | [238_circular_economy_material_tracking.tern](238_circular_economy_material_tracking.tern) | Circular Economy Material Tracking |
| 239 | [239_industrial_emission_control.tern](239_industrial_emission_control.tern) | Industrial Emission Control |
| 240 | [240_nuclear_waste_disposal_safety.tern](240_nuclear_waste_disposal_safety.tern) | Nuclear Waste Disposal Safety |
| 241 | [241_carbon_capture_and_storage.tern](241_carbon_capture_and_storage.tern) | Carbon Capture and Storage |
| 242 | [242_hydrogen_fuel_production.tern](242_hydrogen_fuel_production.tern) | Hydrogen Fuel Production |
| 243 | [243_biofuel_feedstock_optimization.tern](243_biofuel_feedstock_optimization.tern) | Biofuel Feedstock Optimization |
| 244 | [244_geological_carbon_sequestration.tern](244_geological_carbon_sequestration.tern) | Geological Carbon Sequestration |
| 245 | [245_methane_leak_detection.tern](245_methane_leak_detection.tern) | Methane Leak Detection |
| 246 | [246_sustainable_forest_management.tern](246_sustainable_forest_management.tern) | Sustainable Forest Management |
| 247 | [247_eco_label_certification.tern](247_eco_label_certification.tern) | Eco-label Certification |
| 248 | [248_green_bond_verification.tern](248_green_bond_verification.tern) | Green Bond Verification |
| 249 | [249_corporate_esg_reporting.tern](249_corporate_esg_reporting.tern) | Corporate ESG Reporting |
| 250 | [250_supply_chain_human_rights.tern](250_supply_chain_human_rights.tern) | Supply Chain Human Rights |


## Patterns Demonstrated

### Hard Gate
A signal so important that its negative value overrides everything else:
```
match critical_signal {
    -1 => { return conflict(); }   // veto — no further evaluation needed
     0 => { ... }
     1 => { ... }
}
```
Used in: rocket launch (range safety), medical triage (consciousness), spam filter (blocklist), loan underwriting (DTI ratio).

### Density Check
When fewer than N of M signals are decisive, request more data instead of guessing:
```
// if not enough decisive signals → return hold()
// "I don't know yet — here's what I need"
```
Used in: `08_evidence_collector.tern`, `10_confidence_escalator.tern`, `12_vote_aggregator.tern`.

### Cascading Consensus
Chain `consensus(a, b)` calls to aggregate multiple signals:
```
let ab:  trit = consensus(signal_a, signal_b);
let abc: trit = consensus(ab, signal_c);
```
Used in: nearly every example. The workhorse of ternary aggregation.

### Hold as Routing Label
`hold()` is not "undecided" — it is a first-class output value that tells the caller what to do next:
- Spam filter: quarantine folder
- Circuit breaker: probe mode
- Content moderation: human review queue
- Loan underwriting: human underwriter queue
- Form validator: show "required" hint, not error

---

## Contributing

New examples follow the naming convention `NN_snake_case_name.tern`.

Every example should:
1. Have a header comment explaining the real-world scenario
2. Demonstrate what binary systems get wrong (and why ternary fixes it)
3. Have a concrete scenario at the end that can be traced through manually
4. Return a meaningful trit via a top-level `match` block

---

## Attribution

- `09_risc_fetch_decode.tern` — conceptually informed by Brandon Smith's Python 9-trit RISC simulator
- `13_owlet_bridge.tern` — conceptually informed by the Owlet S-expression ternary interpreter (Node.js)
- Balanced ternary mathematical foundations: Knuth (1997), *The Art of Computer Programming*
- Physical ternary precedent: Setun computer, Moscow State University, 1958
- BitNet b1.58 ternary neural network weights: Ma et al. (2024)
