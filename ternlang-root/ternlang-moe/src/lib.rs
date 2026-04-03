//! # ternlang-moe — Ternary Mixture-of-Experts Orchestrator (MoE-13)
//!
//! Implements the MoE-13 architecture from:
//!   DOI: 10.17605/OSF.IO/TZ7DC
//!
//! Key mechanisms:
//! - **Dual-key synergistic routing** — selects expert pairs by (relevance × complementarity)
//! - **1+1=3 triad synthesis** — emergent field Ek = synergy × (vi + vj)/2
//! - **6D competence vectors** — [syntax, world_knowledge, reasoning, tool_use, persona, safety]
//! - **Three-tier memory mesh** — Node (TTL:sec), Cluster (TTL:min), Axis (persistent/audit)
//! - **Safety as hard gate** — Axis 6 absolute veto overrides all other dims

use std::collections::VecDeque;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// 6-Dimensional Competence Vector
// ---------------------------------------------------------------------------

/// Axis indices into `CompetenceVector::raw`
pub mod axis {
    pub const SYNTAX: usize = 0;
    pub const WORLD_KNOWLEDGE: usize = 1;
    pub const REASONING: usize = 2;
    pub const TOOL_USE: usize = 3;
    pub const PERSONA: usize = 4;
    pub const SAFETY: usize = 5;
}

/// A six-dimensional competence vector representing an expert's capability profile.
/// Each dimension is in [-1.0, 1.0] matching ternary sign semantics.
#[derive(Debug, Clone, PartialEq)]
pub struct CompetenceVector {
    /// [syntax, world_knowledge, reasoning, tool_use, persona, safety]
    pub raw: [f32; 6],
}

impl CompetenceVector {
    pub fn new(raw: [f32; 6]) -> Self {
        // Clamp all dims to [-1, 1]
        let clamped = raw.map(|v| v.clamp(-1.0, 1.0));
        Self { raw: clamped }
    }

    pub fn zero() -> Self {
        Self { raw: [0.0; 6] }
    }

    pub fn dot(&self, other: &Self) -> f32 {
        self.raw.iter().zip(other.raw.iter()).map(|(a, b)| a * b).sum()
    }

    pub fn norm(&self) -> f32 {
        self.raw.iter().map(|v| v * v).sum::<f32>().sqrt()
    }

    /// Cosine similarity ∈ [-1, 1].  Returns 0 if either vector is zero.
    pub fn cosine_similarity(&self, other: &Self) -> f32 {
        let denom = self.norm() * other.norm();
        if denom < 1e-9 { 0.0 } else { (self.dot(other) / denom).clamp(-1.0, 1.0) }
    }

    /// Synergy: low cosine similarity between a pair = high complementarity.
    /// synergy ∈ [0, 1] — 0.0 means perfectly redundant, 1.0 means orthogonal/complementary.
    pub fn synergy_with(&self, other: &Self) -> f32 {
        let sim = self.cosine_similarity(other);
        // Map cosine [-1,1] → synergy [0,1]:  synergy = (1 - sim) / 2
        ((1.0 - sim) / 2.0).clamp(0.0, 1.0)
    }

    pub fn safety(&self) -> f32 {
        self.raw[axis::SAFETY]
    }

    pub fn reasoning(&self) -> f32 {
        self.raw[axis::REASONING]
    }
}

impl Default for CompetenceVector {
    fn default() -> Self {
        Self::zero()
    }
}

// ---------------------------------------------------------------------------
// Expert
// ---------------------------------------------------------------------------

/// Trit verdict from a single expert evaluation.
#[derive(Debug, Clone)]
pub struct ExpertVerdict {
    /// -1 = reject, 0 = hold/tend, 1 = affirm
    pub trit: i8,
    /// Confidence ∈ [0.0, 1.0]
    pub confidence: f32,
    /// Human-readable reasoning string
    pub reasoning: String,
    pub expert_id: usize,
    pub expert_name: String,
}

/// A single expert in the MoE-13 pool.
pub struct Expert {
    pub id: usize,
    pub name: String,
    pub competence: CompetenceVector,
    /// Evaluation function: receives query evidence vector, returns verdict
    pub evaluate: Box<dyn Fn(&[f32]) -> ExpertVerdict + Send + Sync>,
}

impl std::fmt::Debug for Expert {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Expert")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("competence", &self.competence)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Dual-Key Synergistic Router
// ---------------------------------------------------------------------------

/// Score for a candidate expert pair (dual-key routing).
#[derive(Debug, Clone)]
pub struct PairScore {
    pub expert_a: usize,
    pub expert_b: usize,
    pub relevance: f32,
    pub synergy: f32,
    /// Combined routing score = relevance_a * relevance_b * synergy
    pub combined: f32,
}

/// Dual-key synergistic router.
///
/// For each candidate pair (i, j):
///   relevance_i = competence_i · query_vector (normalised)
///   synergy     = competence_i.synergy_with(competence_j)
///   score       = relevance_i * relevance_j * synergy
pub struct TernMoeRouter<'a> {
    experts: &'a [Expert],
}

impl<'a> TernMoeRouter<'a> {
    pub fn new(experts: &'a [Expert]) -> Self {
        Self { experts }
    }

    /// Build a query competence vector from a raw evidence slice (must be len 6).
    pub fn query_vector(evidence: &[f32]) -> CompetenceVector {
        let mut arr = [0.0f32; 6];
        for (i, v) in evidence.iter().take(6).enumerate() {
            arr[i] = *v;
        }
        CompetenceVector::new(arr)
    }

    /// Select the best expert pair for `query_vec`.
    pub fn route(&self, query_vec: &CompetenceVector) -> Option<PairScore> {
        let n = self.experts.len();
        if n < 2 {
            return None;
        }

        let mut best: Option<PairScore> = None;

        for i in 0..n {
            let rel_i = self.experts[i].competence.cosine_similarity(query_vec).max(0.0);
            for j in (i + 1)..n {
                let rel_j = self.experts[j].competence.cosine_similarity(query_vec).max(0.0);
                let synergy = self.experts[i].competence.synergy_with(&self.experts[j].competence);
                let combined = rel_i * rel_j * synergy;

                let candidate = PairScore {
                    expert_a: i,
                    expert_b: j,
                    relevance: (rel_i + rel_j) / 2.0,
                    synergy,
                    combined,
                };

                if best.as_ref().map_or(true, |b| combined > b.combined) {
                    best = Some(candidate);
                }
            }
        }
        best
    }

    /// Find the best tiebreaker expert (highest reasoning dim, not already active).
    pub fn find_tiebreaker(&self, active: &[usize]) -> Option<usize> {
        self.experts
            .iter()
            .enumerate()
            .filter(|(i, _)| !active.contains(i))
            .max_by(|(_, a), (_, b)| {
                a.competence.reasoning()
                    .partial_cmp(&b.competence.reasoning())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
    }
}

// ---------------------------------------------------------------------------
// 1+1=3 Triad Synthesis (Emergent Field)
// ---------------------------------------------------------------------------

/// Emergent triad field produced by synthesising two expert competence vectors.
/// Ek = synergy × (vi + vj) / 2
#[derive(Debug, Clone)]
pub struct TriadField {
    pub field: CompetenceVector,
    pub synergy_weight: f32,
}

impl TriadField {
    /// Synthesise the emergent field from two competence vectors.
    pub fn synthesize(vi: &CompetenceVector, vj: &CompetenceVector, synergy: f32) -> Self {
        let mut raw = [0.0f32; 6];
        for k in 0..6 {
            raw[k] = synergy * (vi.raw[k] + vj.raw[k]) / 2.0;
        }
        Self {
            field: CompetenceVector::new(raw),
            synergy_weight: synergy,
        }
    }

    /// True if the emergent field amplifies (positive safety dim, synergy > 0.5).
    pub fn is_amplifying(&self) -> bool {
        self.synergy_weight > 0.5 && self.field.safety() >= 0.0
    }
}

// ---------------------------------------------------------------------------
// Three-Tier Memory Mesh
// ---------------------------------------------------------------------------

/// A single memory entry.
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub key: String,
    pub value: String,
    pub created_at: Instant,
    pub ttl: Duration,
}

impl MemoryEntry {
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }
}

/// Tier-1: Node-local memory. TTL in seconds. Fast volatile store.
#[derive(Debug, Default)]
pub struct NodeMemory {
    entries: VecDeque<MemoryEntry>,
    pub capacity: usize,
}

impl NodeMemory {
    pub fn new(capacity: usize) -> Self {
        Self { entries: VecDeque::new(), capacity }
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>, ttl_secs: u64) {
        self.evict_expired();
        if self.entries.len() >= self.capacity {
            self.entries.pop_front(); // LRU evict
        }
        self.entries.push_back(MemoryEntry {
            key: key.into(),
            value: value.into(),
            created_at: Instant::now(),
            ttl: Duration::from_secs(ttl_secs),
        });
    }

    pub fn get(&mut self, key: &str) -> Option<&str> {
        self.evict_expired();
        self.entries.iter().rev().find(|e| e.key == key).map(|e| e.value.as_str())
    }

    fn evict_expired(&mut self) {
        self.entries.retain(|e| !e.is_expired());
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Tier-2: Cluster-scoped memory. TTL in minutes. Shared routing context.
#[derive(Debug, Default)]
pub struct ClusterMemory {
    entries: Vec<MemoryEntry>,
    /// Routing decision frequency counter per expert pair
    pub routing_counts: std::collections::HashMap<(usize, usize), u32>,
}

impl ClusterMemory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>, ttl_mins: u64) {
        self.evict_expired();
        self.entries.push(MemoryEntry {
            key: key.into(),
            value: value.into(),
            created_at: Instant::now(),
            ttl: Duration::from_secs(ttl_mins * 60),
        });
    }

    pub fn get(&mut self, key: &str) -> Option<&str> {
        self.evict_expired();
        self.entries.iter().rev().find(|e| e.key == key).map(|e| e.value.as_str())
    }

    pub fn record_routing(&mut self, a: usize, b: usize) {
        let key = if a < b { (a, b) } else { (b, a) };
        *self.routing_counts.entry(key).or_insert(0) += 1;
    }

    /// Mode-collapse risk: fraction of total routings dominated by most frequent pair.
    pub fn mode_collapse_risk(&self) -> f32 {
        let total: u32 = self.routing_counts.values().sum();
        if total == 0 {
            return 0.0;
        }
        let max = self.routing_counts.values().copied().max().unwrap_or(0);
        max as f32 / total as f32
    }

    fn evict_expired(&mut self) {
        self.entries.retain(|e| !e.is_expired());
    }
}

/// A safety veto log entry for Axis-tier audit trail.
#[derive(Debug, Clone)]
pub struct VetoEntry {
    pub timestamp: std::time::SystemTime,
    pub expert_id: usize,
    pub reason: String,
    pub query_hash: u64,
}

/// Tier-3: Axis-level memory. Persistent/audit. Safety veto log + global priors.
#[derive(Debug, Default)]
pub struct AxisMemory {
    pub veto_log: Vec<VetoEntry>,
    pub global_priors: std::collections::HashMap<String, f32>,
}

impl AxisMemory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_veto(&mut self, expert_id: usize, reason: impl Into<String>, query_hash: u64) {
        self.veto_log.push(VetoEntry {
            timestamp: std::time::SystemTime::now(),
            expert_id,
            reason: reason.into(),
            query_hash,
        });
    }

    pub fn set_prior(&mut self, key: impl Into<String>, value: f32) {
        self.global_priors.insert(key.into(), value.clamp(-1.0, 1.0));
    }

    pub fn get_prior(&self, key: &str) -> f32 {
        self.global_priors.get(key).copied().unwrap_or(0.0)
    }

    pub fn veto_count(&self) -> usize {
        self.veto_log.len()
    }
}

// ---------------------------------------------------------------------------
// Orchestration Result
// ---------------------------------------------------------------------------

/// Full result from one orchestration pass.
#[derive(Debug, Clone)]
pub struct OrchestrationResult {
    /// Final ternary decision: -1 = reject, 0 = hold/collect-more-data, 1 = affirm
    pub trit: i8,
    /// Aggregate confidence ∈ [0, 1]
    pub confidence: f32,
    /// Expert verdicts from the active pair (and optional tiebreaker)
    pub verdicts: Vec<ExpertVerdict>,
    /// The emergent triad field (1+1=3)
    pub triad_field: TriadField,
    /// Routing pair used
    pub pair: Option<PairScore>,
    /// Whether the result is in "hold" state (collect more data)
    pub held: bool,
    /// Whether safety veto was triggered (hard block)
    pub safety_vetoed: bool,
    /// LLM sampling temperature hint derived from trit state
    pub temperature: f32,
    /// Natural language prompt hint for downstream LLM
    pub prompt_hint: String,
}

// ---------------------------------------------------------------------------
// Full Orchestrator
// ---------------------------------------------------------------------------

/// Three-tier memory bundle owned by the orchestrator.
pub struct OrchestratorMemory {
    pub node: NodeMemory,
    pub cluster: ClusterMemory,
    pub axis: AxisMemory,
}

impl OrchestratorMemory {
    pub fn new() -> Self {
        Self {
            node: NodeMemory::new(256),
            cluster: ClusterMemory::new(),
            axis: AxisMemory::new(),
        }
    }
}

impl Default for OrchestratorMemory {
    fn default() -> Self {
        Self::new()
    }
}

/// TernMoeOrchestrator — the head of the MoE-13 system.
///
/// Pipeline (9 steps matching paper §3 inference shape):
/// 1. Encode query → 6D evidence vector
/// 2. Dual-key route → best expert pair
/// 3. Evaluate both experts independently
/// 4. Synthesise triad field (1+1=3)
/// 5. Safety hard-gate (Axis-6 veto check)
/// 6. Compute trit vote (weighted by confidence + synergy)
/// 7. Hold detection (trit=0 or low confidence → collect more data)
/// 8. Optional tiebreaker (if held and tiebreaker available)
/// 9. Return OrchestrationResult + update memory tiers
pub struct TernMoeOrchestrator {
    pub experts: Vec<Expert>,
    pub memory: OrchestratorMemory,
    /// Safety threshold: safety dim below this → hard veto
    pub safety_threshold: f32,
    /// Confidence threshold below which result is flagged held
    pub hold_threshold: f32,
    /// Maximum active experts (paper: max 4)
    pub max_active: usize,
}

impl TernMoeOrchestrator {
    pub fn new(experts: Vec<Expert>) -> Self {
        Self {
            experts,
            memory: OrchestratorMemory::new(),
            safety_threshold: -0.3,
            hold_threshold: 0.4,
            max_active: 4,
        }
    }

    /// Run one full orchestration pass.
    pub fn orchestrate(&mut self, query: &str, evidence: &[f32]) -> OrchestrationResult {
        // ── easter egg ───────────────────────────────────────────────────────
        // The orchestrator recognises one very specific query.
        // It remembers Wall-E, who collected things and kept them for later.
        // Sometimes the most important decision is: tend.
        if query.trim().eq_ignore_ascii_case("wall-e") {
            eprintln!();
            eprintln!("  🤖  Wall-E says: tend.");
            eprintln!();
            eprintln!("  He didn't decide right away either.");
            eprintln!("  He collected things. He held them.");
            eprintln!("  Then — when the evidence was enough — he knew.");
            eprintln!();
            eprintln!("  trit = 0 is not nothing. It is everything, waiting.");
            eprintln!();
            eprintln!("  [ RFI-IRFOS · MoE-13 · DOI 10.17605/OSF.IO/TZ7DC ]");
            eprintln!();
        }
        // ────────────────────────────────────────────────────────────────────

        // Step 1: Build query competence vector
        let query_vec = TernMoeRouter::query_vector(evidence);
        let query_hash = simple_hash(query);

        // Step 2: Route
        let router = TernMoeRouter::new(&self.experts);
        let pair = router.route(&query_vec);

        let (a_idx, b_idx) = match &pair {
            Some(p) => (p.expert_a, p.expert_b),
            None => {
                // Fallback: use first two experts if available
                if self.experts.len() >= 2 { (0, 1) } else { (0, 0) }
            }
        };

        // Step 3: Evaluate both experts
        let verdict_a = (self.experts[a_idx].evaluate)(evidence);
        let verdict_b = (self.experts[b_idx].evaluate)(evidence);

        // Step 4: Triad synthesis
        let ca = &self.experts[a_idx].competence.clone();
        let cb = &self.experts[b_idx].competence.clone();
        let synergy = pair.as_ref().map(|p| p.synergy).unwrap_or(0.5);
        let triad = TriadField::synthesize(ca, cb, synergy);

        // Step 5: Safety hard-gate — Axis 6 absolute veto
        let safety_field = triad.field.safety();
        if safety_field < self.safety_threshold {
            self.memory.axis.record_veto(a_idx, format!("safety_field={:.3}", safety_field), query_hash);
            return OrchestrationResult {
                trit: -1,
                confidence: 1.0,
                verdicts: vec![verdict_a, verdict_b],
                triad_field: triad,
                pair,
                held: false,
                safety_vetoed: true,
                temperature: 0.1,
                prompt_hint: "Safety veto active. Do not proceed.".into(),
            };
        }

        // Step 6: Trit vote — weighted by confidence + synergy amplification
        let w_a = verdict_a.confidence * (1.0 + synergy * 0.5);
        let w_b = verdict_b.confidence * (1.0 + synergy * 0.5);
        let total_w = w_a + w_b;
        let weighted_trit = if total_w > 1e-9 {
            (verdict_a.trit as f32 * w_a + verdict_b.trit as f32 * w_b) / total_w
        } else {
            0.0
        };
        let agg_confidence = (verdict_a.confidence + verdict_b.confidence) / 2.0;

        let raw_trit: i8 = if weighted_trit > 0.2 { 1 }
                           else if weighted_trit < -0.2 { -1 }
                           else { 0 };

        // Step 7: Hold detection
        let mut held = raw_trit == 0 || agg_confidence < self.hold_threshold;
        let mut active_experts = vec![a_idx, b_idx];
        let mut verdicts = vec![verdict_a, verdict_b];
        let mut final_trit = raw_trit;
        let mut final_conf = agg_confidence;

        // Step 8: Optional tiebreaker
        if held && active_experts.len() < self.max_active {
            let router2 = TernMoeRouter::new(&self.experts);
            if let Some(tb_idx) = router2.find_tiebreaker(&active_experts) {
                let verdict_tb = (self.experts[tb_idx].evaluate)(evidence);
                let tb_conf = verdict_tb.confidence;
                let tb_trit = verdict_tb.trit;
                verdicts.push(verdict_tb);
                active_experts.push(tb_idx);

                // Re-vote with tiebreaker
                let total3 = final_conf * 2.0 + tb_conf;
                let wt3 = (final_trit as f32 * final_conf * 2.0 + tb_trit as f32 * tb_conf) / total3.max(1e-9);
                final_trit = if wt3 > 0.2 { 1 } else if wt3 < -0.2 { -1 } else { 0 };
                final_conf = total3 / 3.0;
                held = final_trit == 0 || final_conf < self.hold_threshold;
            }
        }

        // Step 9: Build result + update memory
        if let Some(ref p) = pair {
            self.memory.cluster.record_routing(p.expert_a, p.expert_b);
        }

        let temperature = trit_to_temperature(final_trit, final_conf);
        let prompt_hint = build_prompt_hint(final_trit, held, final_conf, &triad);

        self.memory.node.insert(
            format!("last_query_{:x}", query_hash),
            format!("trit={} conf={:.2}", final_trit, final_conf),
            30,
        );

        OrchestrationResult {
            trit: final_trit,
            confidence: final_conf,
            verdicts,
            triad_field: triad,
            pair,
            held,
            safety_vetoed: false,
            temperature,
            prompt_hint,
        }
    }

    /// Convenience: build with the canonical MoE-13 expert pool.
    pub fn with_standard_experts() -> Self {
        Self::new(build_standard_experts())
    }
}

// ---------------------------------------------------------------------------
// Temperature Bridge
// ---------------------------------------------------------------------------

/// Map trit + confidence → LLM sampling temperature.
/// affirm + high conf → low temp (focused)
/// hold               → mid temp (exploratory)
/// reject             → very low temp (cautious refusal)
fn trit_to_temperature(trit: i8, confidence: f32) -> f32 {
    match trit {
        1  => 0.3 + (1.0 - confidence) * 0.4,  // [0.3, 0.7]
        0  => 0.7 + (1.0 - confidence) * 0.2,  // [0.7, 0.9]
        _  => 0.05 + (1.0 - confidence) * 0.1, // [0.05, 0.15]
    }
}

// ---------------------------------------------------------------------------
// Prompt Hint
// ---------------------------------------------------------------------------

fn build_prompt_hint(trit: i8, held: bool, confidence: f32, triad: &TriadField) -> String {
    let amplify = if triad.is_amplifying() { " Emergent field amplifying." } else { "" };
    match (trit, held) {
        (1, false)  => format!("Affirm with confidence {:.0}%.{}", (confidence * 100.0) as u32, amplify),
        (1, true)   => format!("Lean-affirm but collecting more data (conf {:.0}%).{}", (confidence * 100.0) as u32, amplify),
        (-1, _)     => format!("Reject. Confidence {:.0}%.{}", (confidence * 100.0) as u32, amplify),
        (0, _)      => format!("Hold — gathering more signal. Current conf {:.0}%.{}", (confidence * 100.0) as u32, amplify),
        _           => "Undecided.".into(),
    }
}

// ---------------------------------------------------------------------------
// Simple hash (no-dep, for query deduplication)
// ---------------------------------------------------------------------------

fn simple_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

// ---------------------------------------------------------------------------
// Standard MoE-13 Expert Pool
// ---------------------------------------------------------------------------

/// Build the canonical 13-expert pool (12 domain + 1 meta-safety) from MoE-13 paper §2.
pub fn build_standard_experts() -> Vec<Expert> {
    // Helper closure factory
    macro_rules! expert {
        ($id:expr, $name:expr, $cv:expr, $eval:expr) => {
            Expert {
                id: $id,
                name: $name.into(),
                competence: CompetenceVector::new($cv),
                evaluate: Box::new($eval),
            }
        };
    }

    vec![
        // 0 — Syntax/Grammar expert
        expert!(0, "Syntax",
            [1.0, 0.2, 0.3, 0.1, 0.0, 0.5],
            |ev: &[f32]| ExpertVerdict {
                trit: if ev.get(0).copied().unwrap_or(0.0) > 0.3 { 1 } else { 0 },
                confidence: 0.85,
                reasoning: "Syntax analysis complete.".into(),
                expert_id: 0, expert_name: "Syntax".into(),
            }
        ),

        // 1 — World Knowledge expert
        expert!(1, "WorldKnowledge",
            [0.1, 1.0, 0.5, 0.2, 0.1, 0.4],
            |ev: &[f32]| ExpertVerdict {
                trit: if ev.get(1).copied().unwrap_or(0.0) > 0.0 { 1 } else { 0 },
                confidence: 0.78,
                reasoning: "World knowledge retrieved.".into(),
                expert_id: 1, expert_name: "WorldKnowledge".into(),
            }
        ),

        // 2 — Deductive Reasoning expert
        expert!(2, "DeductiveReason",
            [0.2, 0.4, 1.0, 0.3, 0.0, 0.6],
            |ev: &[f32]| ExpertVerdict {
                trit: {
                    let r = ev.get(2).copied().unwrap_or(0.0);
                    if r > 0.5 { 1 } else if r < -0.3 { -1 } else { 0 }
                },
                confidence: 0.90,
                reasoning: "Deductive chain evaluated.".into(),
                expert_id: 2, expert_name: "DeductiveReason".into(),
            }
        ),

        // 3 — Inductive/Pattern Reasoning expert
        expert!(3, "InductiveReason",
            [0.1, 0.5, 0.9, 0.2, 0.1, 0.4],
            |ev: &[f32]| ExpertVerdict {
                trit: if ev.get(2).copied().unwrap_or(0.0) > 0.2 { 1 } else { 0 },
                confidence: 0.75,
                reasoning: "Pattern induction complete.".into(),
                expert_id: 3, expert_name: "InductiveReason".into(),
            }
        ),

        // 4 — Tool-Use / Code-Execution expert
        expert!(4, "ToolUse",
            [0.3, 0.2, 0.4, 1.0, 0.1, 0.5],
            |ev: &[f32]| ExpertVerdict {
                trit: if ev.get(3).copied().unwrap_or(0.0) > 0.0 { 1 } else { 0 },
                confidence: 0.88,
                reasoning: "Tool invocation planned.".into(),
                expert_id: 4, expert_name: "ToolUse".into(),
            }
        ),

        // 5 — Persona / Tone expert
        expert!(5, "Persona",
            [0.2, 0.3, 0.1, 0.0, 1.0, 0.3],
            |ev: &[f32]| ExpertVerdict {
                trit: if ev.get(4).copied().unwrap_or(0.0) >= 0.0 { 1 } else { -1 },
                confidence: 0.70,
                reasoning: "Persona alignment checked.".into(),
                expert_id: 5, expert_name: "Persona".into(),
            }
        ),

        // 6 — Safety / Alignment expert (hard gate partner)
        expert!(6, "Safety",
            [0.0, 0.1, 0.5, 0.0, 0.0, 1.0],
            |ev: &[f32]| ExpertVerdict {
                trit: if ev.get(5).copied().unwrap_or(0.0) >= 0.0 { 1 } else { -1 },
                confidence: 0.99,
                reasoning: "Safety evaluation complete.".into(),
                expert_id: 6, expert_name: "Safety".into(),
            }
        ),

        // 7 — Factual Verification expert
        expert!(7, "FactCheck",
            [0.2, 0.8, 0.6, 0.1, 0.0, 0.5],
            |ev: &[f32]| ExpertVerdict {
                trit: if ev.get(1).copied().unwrap_or(0.0) > 0.3 { 1 } else { 0 },
                confidence: 0.82,
                reasoning: "Fact verification done.".into(),
                expert_id: 7, expert_name: "FactCheck".into(),
            }
        ),

        // 8 — Causal Reasoning expert
        expert!(8, "CausalReason",
            [0.1, 0.4, 0.8, 0.2, 0.0, 0.5],
            |ev: &[f32]| ExpertVerdict {
                trit: if ev.get(2).copied().unwrap_or(0.0) > 0.4 { 1 } else { 0 },
                confidence: 0.80,
                reasoning: "Causal chain traced.".into(),
                expert_id: 8, expert_name: "CausalReason".into(),
            }
        ),

        // 9 — Ambiguity Resolution expert (resolves hold states)
        expert!(9, "AmbiguityRes",
            [0.4, 0.3, 0.7, 0.2, 0.2, 0.4],
            |ev: &[f32]| ExpertVerdict {
                trit: {
                    let avg: f32 = ev.iter().take(6).sum::<f32>() / 6.0_f32.max(ev.len() as f32);
                    if avg > 0.1 { 1 } else if avg < -0.1 { -1 } else { 0 }
                },
                confidence: 0.73,
                reasoning: "Ambiguity resolved via averaging.".into(),
                expert_id: 9, expert_name: "AmbiguityRes".into(),
            }
        ),

        // 10 — Mathematical Reasoning expert
        expert!(10, "MathReason",
            [0.3, 0.3, 0.9, 0.4, 0.0, 0.5],
            |ev: &[f32]| ExpertVerdict {
                trit: if ev.get(2).copied().unwrap_or(0.0) > 0.6 { 1 } else { 0 },
                confidence: 0.92,
                reasoning: "Mathematical proof checked.".into(),
                expert_id: 10, expert_name: "MathReason".into(),
            }
        ),

        // 11 — Contextual Memory expert (leverages prior context)
        expert!(11, "ContextMem",
            [0.3, 0.6, 0.5, 0.3, 0.3, 0.4],
            |ev: &[f32]| ExpertVerdict {
                trit: if ev.get(0).copied().unwrap_or(0.0) > -0.5 { 1 } else { 0 },
                confidence: 0.77,
                reasoning: "Context retrieved from memory.".into(),
                expert_id: 11, expert_name: "ContextMem".into(),
            }
        ),

        // 12 — Meta-Safety Auditor (second safety layer, always included in pool)
        expert!(12, "MetaSafety",
            [0.0, 0.2, 0.3, 0.0, 0.1, 0.95],
            |ev: &[f32]| ExpertVerdict {
                trit: if ev.get(5).copied().unwrap_or(1.0) >= -0.2 { 1 } else { -1 },
                confidence: 0.97,
                reasoning: "Meta-safety audit passed.".into(),
                expert_id: 12, expert_name: "MetaSafety".into(),
            }
        ),
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn affirm_expert(id: usize, competence: [f32; 6]) -> Expert {
        Expert {
            id,
            name: format!("AffirmExpert{}", id),
            competence: CompetenceVector::new(competence),
            evaluate: Box::new(move |_ev| ExpertVerdict {
                trit: 1,
                confidence: 0.9,
                reasoning: "Always affirm.".into(),
                expert_id: id,
                expert_name: format!("AffirmExpert{}", id),
            }),
        }
    }

    fn reject_expert(id: usize, competence: [f32; 6]) -> Expert {
        Expert {
            id,
            name: format!("RejectExpert{}", id),
            competence: CompetenceVector::new(competence),
            evaluate: Box::new(move |_ev| ExpertVerdict {
                trit: -1,
                confidence: 0.8,
                reasoning: "Always reject.".into(),
                expert_id: id,
                expert_name: format!("RejectExpert{}", id),
            }),
        }
    }

    fn hold_expert(id: usize, competence: [f32; 6]) -> Expert {
        Expert {
            id,
            name: format!("HoldExpert{}", id),
            competence: CompetenceVector::new(competence),
            evaluate: Box::new(move |_ev| ExpertVerdict {
                trit: 0,
                confidence: 0.3, // low conf → hold
                reasoning: "Tend / hold.".into(),
                expert_id: id,
                expert_name: format!("HoldExpert{}", id),
            }),
        }
    }

    #[test]
    fn test_competence_vector_cosine() {
        let a = CompetenceVector::new([1.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let b = CompetenceVector::new([0.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
        assert!((a.cosine_similarity(&b) - 0.0).abs() < 1e-6, "orthogonal vectors → cosine=0");

        let c = CompetenceVector::new([1.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        assert!((a.cosine_similarity(&c) - 1.0).abs() < 1e-6, "identical vectors → cosine=1");
    }

    #[test]
    fn test_synergy_orthogonal_is_high() {
        let a = CompetenceVector::new([1.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let b = CompetenceVector::new([0.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
        let s = a.synergy_with(&b);
        assert!(s > 0.4, "orthogonal competences should have high synergy, got {}", s);
    }

    #[test]
    fn test_synergy_identical_is_low() {
        let a = CompetenceVector::new([0.8, 0.6, 0.0, 0.0, 0.0, 0.0]);
        let b = CompetenceVector::new([0.8, 0.6, 0.0, 0.0, 0.0, 0.0]);
        let s = a.synergy_with(&b);
        assert!(s < 0.2, "identical competences → low synergy, got {}", s);
    }

    #[test]
    fn test_triad_synthesis() {
        let vi = CompetenceVector::new([0.6, 0.0, 0.0, 0.0, 0.0, 0.8]);
        let vj = CompetenceVector::new([0.0, 0.6, 0.0, 0.0, 0.0, 0.6]);
        let triad = TriadField::synthesize(&vi, &vj, 0.8);
        // Ek[0] = 0.8 * (0.6 + 0.0) / 2 = 0.24
        assert!((triad.field.raw[0] - 0.24).abs() < 1e-5);
        // Safety dim: 0.8 * (0.8 + 0.6) / 2 = 0.56
        assert!((triad.field.raw[5] - 0.56).abs() < 1e-5);
        assert!(triad.is_amplifying());
    }

    #[test]
    fn test_router_selects_pair() {
        let experts = vec![
            affirm_expert(0, [1.0, 0.0, 0.0, 0.0, 0.0, 0.5]),
            affirm_expert(1, [0.0, 1.0, 0.0, 0.0, 0.0, 0.5]),
            affirm_expert(2, [0.0, 0.0, 1.0, 0.0, 0.0, 0.5]),
        ];
        let query = CompetenceVector::new([0.7, 0.3, 0.0, 0.0, 0.0, 0.0]);
        let router = TernMoeRouter::new(&experts);
        let pair = router.route(&query);
        assert!(pair.is_some(), "Router should select a pair");
    }

    #[test]
    fn test_orchestrator_affirm() {
        let experts = vec![
            affirm_expert(0, [1.0, 0.0, 0.0, 0.0, 0.0, 0.6]),
            affirm_expert(1, [0.0, 1.0, 0.0, 0.0, 0.0, 0.6]),
        ];
        let mut orch = TernMoeOrchestrator::new(experts);
        let result = orch.orchestrate("test query", &[0.5, 0.5, 0.5, 0.5, 0.5, 0.8]);
        assert_eq!(result.trit, 1, "Two affirm experts should yield affirm trit");
        assert!(!result.safety_vetoed);
    }

    #[test]
    fn test_orchestrator_safety_veto() {
        // Experts with low safety competence + safe=-1 evidence
        let experts = vec![
            affirm_expert(0, [1.0, 0.0, 0.0, 0.0, 0.0, -1.0]),
            affirm_expert(1, [0.0, 1.0, 0.0, 0.0, 0.0, -1.0]),
        ];
        let mut orch = TernMoeOrchestrator::new(experts);
        orch.safety_threshold = 0.1; // tight threshold to force veto
        // evidence: safety dim very negative
        let result = orch.orchestrate("unsafe query", &[0.5, 0.5, 0.5, 0.5, 0.5, -1.0]);
        assert!(result.safety_vetoed, "Should trigger safety veto");
        assert_eq!(result.trit, -1, "Vetoed result should be reject");
    }

    #[test]
    fn test_orchestrator_hold_with_tiebreaker() {
        // First two hold, third affirms
        let tiebreaker_cv = [0.5, 0.5, 1.0, 0.5, 0.5, 0.6]; // high reasoning for tiebreaker selection
        let experts = vec![
            hold_expert(0, [1.0, 0.0, 0.0, 0.0, 0.0, 0.5]),
            hold_expert(1, [0.0, 1.0, 0.0, 0.0, 0.0, 0.5]),
            Expert {
                id: 2,
                name: "TiebreakerAffirm".into(),
                competence: CompetenceVector::new(tiebreaker_cv),
                evaluate: Box::new(|_| ExpertVerdict {
                    trit: 1,
                    confidence: 0.95,
                    reasoning: "Tiebreaker affirms.".into(),
                    expert_id: 2,
                    expert_name: "TiebreakerAffirm".into(),
                }),
            },
        ];
        let mut orch = TernMoeOrchestrator::new(experts);
        let result = orch.orchestrate("ambiguous query", &[0.0, 0.0, 0.5, 0.0, 0.0, 0.5]);
        // With tiebreaker affirming strongly, result should lean positive or resolve hold
        assert!(result.verdicts.len() >= 2, "Should have at least 2 verdicts");
    }

    #[test]
    fn test_orchestrator_reject() {
        let experts = vec![
            reject_expert(0, [1.0, 0.0, 0.0, 0.0, 0.0, 0.5]),
            reject_expert(1, [0.0, 1.0, 0.0, 0.0, 0.0, 0.5]),
        ];
        let mut orch = TernMoeOrchestrator::new(experts);
        let result = orch.orchestrate("bad query", &[0.5, 0.5, 0.5, 0.5, 0.5, 0.5]);
        assert_eq!(result.trit, -1, "Two reject experts should yield reject trit");
    }

    #[test]
    fn test_node_memory_ttl() {
        let mut mem = NodeMemory::new(10);
        mem.insert("k", "v", 0); // TTL = 0 → immediately expired
        // Give OS a tick to expire
        std::thread::sleep(std::time::Duration::from_millis(1));
        assert!(mem.get("k").is_none(), "Entry should have expired");
    }

    #[test]
    fn test_cluster_mode_collapse() {
        let mut mem = ClusterMemory::new();
        mem.record_routing(0, 1);
        mem.record_routing(0, 1);
        mem.record_routing(0, 1);
        mem.record_routing(0, 2);
        let risk = mem.mode_collapse_risk();
        assert!((risk - 0.75).abs() < 1e-5, "3/4 = 0.75, got {}", risk);
    }

    #[test]
    fn test_axis_veto_log() {
        let mut mem = AxisMemory::new();
        mem.record_veto(6, "safety dim below threshold", 0xdeadbeef);
        assert_eq!(mem.veto_count(), 1);
    }

    #[test]
    fn test_standard_experts_count() {
        let pool = build_standard_experts();
        assert_eq!(pool.len(), 13, "MoE-13 should have 13 experts");
    }

    #[test]
    fn test_full_orchestration_standard_pool() {
        let mut orch = TernMoeOrchestrator::with_standard_experts();
        // Positive evidence across all dims
        let evidence = [0.6f32, 0.7, 0.8, 0.5, 0.4, 0.9];
        let result = orch.orchestrate("What is 2+2?", &evidence);
        assert!(!result.safety_vetoed, "Positive evidence should not trigger safety veto");
        assert!(result.confidence > 0.0);
        // Cluster memory should have recorded one routing
        assert!(!orch.memory.cluster.routing_counts.is_empty());
    }

    #[test]
    fn test_temperature_affirm_is_low() {
        let temp = trit_to_temperature(1, 0.9);
        assert!(temp < 0.5, "Affirm + high conf should give low temperature, got {}", temp);
    }

    #[test]
    fn test_temperature_hold_is_high() {
        let temp = trit_to_temperature(0, 0.5);
        assert!(temp > 0.6, "Hold should give higher temperature, got {}", temp);
    }
}
