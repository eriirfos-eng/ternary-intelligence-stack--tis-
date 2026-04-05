/// StdlibLoader / ModuleResolver — resolves `use` statements into parsed function definitions.
///
/// Two resolution strategies, tried in order:
///   1. Built-in stdlib (embedded at compile time via `include_str!`) — zero filesystem I/O.
///   2. User-defined modules: looks for `<segment>/<segment>.tern` relative to the source file.
///
/// Use `StdlibLoader::resolve()` for quick stdlib-only resolution (backwards-compat API).
/// Use `ModuleResolver::from_source_file(path).resolve(program)` for full module support.
use crate::ast::{Function, Program, Stmt};
use crate::parser::Parser;

// ─── Built-in stdlib sources (compile-time embedded) ─────────────────────────

fn stdlib_source_for(path: &[String]) -> Option<&'static str> {
    match path.join("::").as_str() {
        "agents::coordinator" => Some(include_str!("../../stdlib/agents/coordinator.tern")),
        "agents::debate" => Some(include_str!("../../stdlib/agents/debate.tern")),
        "agents::memory" => Some(include_str!("../../stdlib/agents/memory.tern")),
        "agents::planner" => Some(include_str!("../../stdlib/agents/planner.tern")),
        "agents::reflection" => Some(include_str!("../../stdlib/agents/reflection.tern")),
        "agents::tool_use" => Some(include_str!("../../stdlib/agents/tool_use.tern")),
        "apps::anomaly_detector" => Some(include_str!("../../stdlib/apps/anomaly_detector.tern")),
        "apps::classifier" => Some(include_str!("../../stdlib/apps/classifier.tern")),
        "apps::multi_agent_consensus" => Some(include_str!("../../stdlib/apps/multi_agent_consensus.tern")),
        "apps::sentiment_engine" => Some(include_str!("../../stdlib/apps/sentiment_engine.tern")),
        "apps::sparse_inference_engine" => Some(include_str!("../../stdlib/apps/sparse_inference_engine.tern")),
        "apps::ternary_qa" => Some(include_str!("../../stdlib/apps/ternary_qa.tern")),
        "apps::ternary_rl_agent" => Some(include_str!("../../stdlib/apps/ternary_rl_agent.tern")),
        "benchmarks::agent_throughput" => Some(include_str!("../../stdlib/benchmarks/agent_throughput.tern")),
        "benchmarks::deliberation_speed" => Some(include_str!("../../stdlib/benchmarks/deliberation_speed.tern")),
        "benchmarks::encoding_efficiency" => Some(include_str!("../../stdlib/benchmarks/encoding_efficiency.tern")),
        "benchmarks::sparse_matmul" => Some(include_str!("../../stdlib/benchmarks/sparse_matmul.tern")),
        "bio::codon_map" => Some(include_str!("../../stdlib/bio/codon_map.tern")),
        "bio::mutation_track" => Some(include_str!("../../stdlib/bio/mutation_track.tern")),
        "bio::phylogeny" => Some(include_str!("../../stdlib/bio/phylogeny.tern")),
        "bio::sequence_align" => Some(include_str!("../../stdlib/bio/sequence_align.tern")),
        "classical::cross_validation" => Some(include_str!("../../stdlib/classical/cross_validation.tern")),
        "classical::dbscan" => Some(include_str!("../../stdlib/classical/dbscan.tern")),
        "classical::decision_tree" => Some(include_str!("../../stdlib/classical/decision_tree.tern")),
        "classical::feature_selection" => Some(include_str!("../../stdlib/classical/feature_selection.tern")),
        "classical::gradient_boosting" => Some(include_str!("../../stdlib/classical/gradient_boosting.tern")),
        "classical::kmeans" => Some(include_str!("../../stdlib/classical/kmeans.tern")),
        "classical::knn" => Some(include_str!("../../stdlib/classical/knn.tern")),
        "classical::linear_regression" => Some(include_str!("../../stdlib/classical/linear_regression.tern")),
        "classical::logistic_regression" => Some(include_str!("../../stdlib/classical/logistic_regression.tern")),
        "classical::metrics" => Some(include_str!("../../stdlib/classical/metrics.tern")),
        "classical::naive_bayes" => Some(include_str!("../../stdlib/classical/naive_bayes.tern")),
        "classical::pca" => Some(include_str!("../../stdlib/classical/pca.tern")),
        "classical::preprocessing" => Some(include_str!("../../stdlib/classical/preprocessing.tern")),
        "classical::random_forest" => Some(include_str!("../../stdlib/classical/random_forest.tern")),
        "classical::svm" => Some(include_str!("../../stdlib/classical/svm.tern")),
        "crypto::merkle_trit" => Some(include_str!("../../stdlib/crypto/merkle_trit.tern")),
        "crypto::stream_cipher" => Some(include_str!("../../stdlib/crypto/stream_cipher.tern")),
        "crypto::ternary_hash" => Some(include_str!("../../stdlib/crypto/ternary_hash.tern")),
        "data::batching" => Some(include_str!("../../stdlib/data/batching.tern")),
        "data::drift_detect" => Some(include_str!("../../stdlib/data/drift_detect.tern")),
        "data::feature_store" => Some(include_str!("../../stdlib/data/feature_store.tern")),
        "data::imputation" => Some(include_str!("../../stdlib/data/imputation.tern")),
        "data::normalization" => Some(include_str!("../../stdlib/data/normalization.tern")),
        "data::outlier_detect" => Some(include_str!("../../stdlib/data/outlier_detect.tern")),
        "data::schema" => Some(include_str!("../../stdlib/data/schema.tern")),
        "data::splitting" => Some(include_str!("../../stdlib/data/splitting.tern")),
        "distributed::data_parallel" => Some(include_str!("../../stdlib/distributed/data_parallel.tern")),
        "distributed::federated" => Some(include_str!("../../stdlib/distributed/federated.tern")),
        "distributed::gossip" => Some(include_str!("../../stdlib/distributed/gossip.tern")),
        "distributed::model_parallel" => Some(include_str!("../../stdlib/distributed/model_parallel.tern")),
        "ensemble::bagging" => Some(include_str!("../../stdlib/ensemble/bagging.tern")),
        "ensemble::boosting" => Some(include_str!("../../stdlib/ensemble/boosting.tern")),
        "ensemble::snapshot_ensemble" => Some(include_str!("../../stdlib/ensemble/snapshot_ensemble.tern")),
        "ensemble::stacking" => Some(include_str!("../../stdlib/ensemble/stacking.tern")),
        "ensemble::voting" => Some(include_str!("../../stdlib/ensemble/voting.tern")),
        "eval::calibration_curve" => Some(include_str!("../../stdlib/eval/calibration_curve.tern")),
        "eval::classification_metrics" => Some(include_str!("../../stdlib/eval/classification_metrics.tern")),
        "eval::confusion_matrix" => Some(include_str!("../../stdlib/eval/confusion_matrix.tern")),
        "eval::nlp_metrics" => Some(include_str!("../../stdlib/eval/nlp_metrics.tern")),
        "eval::pr_curve" => Some(include_str!("../../stdlib/eval/pr_curve.tern")),
        "eval::ranking_metrics" => Some(include_str!("../../stdlib/eval/ranking_metrics.tern")),
        "eval::regression_metrics" => Some(include_str!("../../stdlib/eval/regression_metrics.tern")),
        "eval::roc_curve" => Some(include_str!("../../stdlib/eval/roc_curve.tern")),
        "finance::arbitrage" => Some(include_str!("../../stdlib/finance/arbitrage.tern")),
        "finance::momentum" => Some(include_str!("../../stdlib/finance/momentum.tern")),
        "finance::moving_average" => Some(include_str!("../../stdlib/finance/moving_average.tern")),
        "finance::order_book" => Some(include_str!("../../stdlib/finance/order_book.tern")),
        "finance::risk_metrics" => Some(include_str!("../../stdlib/finance/risk_metrics.tern")),
        "gametheory::auction" => Some(include_str!("../../stdlib/gametheory/auction.tern")),
        "gametheory::nash_eq" => Some(include_str!("../../stdlib/gametheory/nash_eq.tern")),
        "gametheory::prisoners_dilemma" => Some(include_str!("../../stdlib/gametheory/prisoners_dilemma.tern")),
        "gametheory::voting_power" => Some(include_str!("../../stdlib/gametheory/voting_power.tern")),
        "graph::community" => Some(include_str!("../../stdlib/graph/community.tern")),
        "graph::gnn_layer" => Some(include_str!("../../stdlib/graph/gnn_layer.tern")),
        "graph::graph_algorithms" => Some(include_str!("../../stdlib/graph/graph_algorithms.tern")),
        "graph::graph_attention" => Some(include_str!("../../stdlib/graph/graph_attention.tern")),
        "graph::graph_core" => Some(include_str!("../../stdlib/graph/graph_core.tern")),
        "graph::graph_sage" => Some(include_str!("../../stdlib/graph/graph_sage.tern")),
        "graph::knowledge_graph" => Some(include_str!("../../stdlib/graph/knowledge_graph.tern")),
        "graph::pagerank" => Some(include_str!("../../stdlib/graph/pagerank.tern")),
        "hardware::alu_trit" => Some(include_str!("../../stdlib/hardware/alu_trit.tern")),
        "hardware::flipflop_trit" => Some(include_str!("../../stdlib/hardware/flipflop_trit.tern")),
        "hardware::memory_bus" => Some(include_str!("../../stdlib/hardware/memory_bus.tern")),
        "hardware::mux_trit" => Some(include_str!("../../stdlib/hardware/mux_trit.tern")),
        "hardware::register_file" => Some(include_str!("../../stdlib/hardware/register_file.tern")),
        "integrations::curl_examples" => Some(include_str!("../../stdlib/integrations/curl_examples.tern")),
        "integrations::mcp_tool_use" => Some(include_str!("../../stdlib/integrations/mcp_tool_use.tern")),
        "integrations::python_bridge" => Some(include_str!("../../stdlib/integrations/python_bridge.tern")),
        "ml::inference" => Some(include_str!("../../stdlib/ml/inference.tern")),
        "ml::layers::attention" => Some(include_str!("../../stdlib/ml/layers/attention.tern")),
        "ml::layers::conv" => Some(include_str!("../../stdlib/ml/layers/conv.tern")),
        "ml::layers::embedding" => Some(include_str!("../../stdlib/ml/layers/embedding.tern")),
        "ml::layers::linear" => Some(include_str!("../../stdlib/ml/layers/linear.tern")),
        "ml::layers::norm" => Some(include_str!("../../stdlib/ml/layers/norm.tern")),
        "ml::layers::recurrent" => Some(include_str!("../../stdlib/ml/layers/recurrent.tern")),
        "ml::layers::transformer" => Some(include_str!("../../stdlib/ml/layers/transformer.tern")),
        "ml::loss::cross_entropy" => Some(include_str!("../../stdlib/ml/loss/cross_entropy.tern")),
        "ml::optim::adam" => Some(include_str!("../../stdlib/ml/optim/adam.tern")),
        "ml::optim::sgd" => Some(include_str!("../../stdlib/ml/optim/sgd.tern")),
        "ml::quantize" => Some(include_str!("../../stdlib/ml/quantize.tern")),
        "ml::train::loop" => Some(include_str!("../../stdlib/ml/train/loop.tern")),
        "models::bert_analog" => Some(include_str!("../../stdlib/models/bert_analog.tern")),
        "models::cnn" => Some(include_str!("../../stdlib/models/cnn.tern")),
        "models::diffusion_analog" => Some(include_str!("../../stdlib/models/diffusion_analog.tern")),
        "models::gan_analog" => Some(include_str!("../../stdlib/models/gan_analog.tern")),
        "models::gpt_analog" => Some(include_str!("../../stdlib/models/gpt_analog.tern")),
        "models::mlp" => Some(include_str!("../../stdlib/models/mlp.tern")),
        "models::resnet_analog" => Some(include_str!("../../stdlib/models/resnet_analog.tern")),
        "models::rnn" => Some(include_str!("../../stdlib/models/rnn.tern")),
        "models::seq2seq" => Some(include_str!("../../stdlib/models/seq2seq.tern")),
        "models::vae_analog" => Some(include_str!("../../stdlib/models/vae_analog.tern")),
        "nlp::beam_search" => Some(include_str!("../../stdlib/nlp/beam_search.tern")),
        "nlp::embeddings" => Some(include_str!("../../stdlib/nlp/embeddings.tern")),
        "nlp::ner" => Some(include_str!("../../stdlib/nlp/ner.tern")),
        "nlp::positional" => Some(include_str!("../../stdlib/nlp/positional.tern")),
        "nlp::sentiment" => Some(include_str!("../../stdlib/nlp/sentiment.tern")),
        "nlp::summarizer" => Some(include_str!("../../stdlib/nlp/summarizer.tern")),
        "nlp::text_classifier" => Some(include_str!("../../stdlib/nlp/text_classifier.tern")),
        "nlp::tokenizer" => Some(include_str!("../../stdlib/nlp/tokenizer.tern")),
        "nlp::translation_gate" => Some(include_str!("../../stdlib/nlp/translation_gate.tern")),
        "nlp::vocab" => Some(include_str!("../../stdlib/nlp/vocab.tern")),
        "nn::activation" => Some(include_str!("../../stdlib/nn/activation.tern")),
        "nn::batch_norm" => Some(include_str!("../../stdlib/nn/batch_norm.tern")),
        "nn::conv1d" => Some(include_str!("../../stdlib/nn/conv1d.tern")),
        "nn::conv2d" => Some(include_str!("../../stdlib/nn/conv2d.tern")),
        "nn::dense" => Some(include_str!("../../stdlib/nn/dense.tern")),
        "nn::dropout" => Some(include_str!("../../stdlib/nn/dropout.tern")),
        "nn::embedding_table" => Some(include_str!("../../stdlib/nn/embedding_table.tern")),
        "nn::feedforward" => Some(include_str!("../../stdlib/nn/feedforward.tern")),
        "nn::gru_cell" => Some(include_str!("../../stdlib/nn/gru_cell.tern")),
        "nn::layer_norm" => Some(include_str!("../../stdlib/nn/layer_norm.tern")),
        "nn::loss::bce" => Some(include_str!("../../stdlib/nn/loss/bce.tern")),
        "nn::loss::contrastive" => Some(include_str!("../../stdlib/nn/loss/contrastive.tern")),
        "nn::loss::focal" => Some(include_str!("../../stdlib/nn/loss/focal.tern")),
        "nn::loss::huber" => Some(include_str!("../../stdlib/nn/loss/huber.tern")),
        "nn::loss::kl_div" => Some(include_str!("../../stdlib/nn/loss/kl_div.tern")),
        "nn::loss::mae" => Some(include_str!("../../stdlib/nn/loss/mae.tern")),
        "nn::loss::mse" => Some(include_str!("../../stdlib/nn/loss/mse.tern")),
        "nn::loss::triplet" => Some(include_str!("../../stdlib/nn/loss/triplet.tern")),
        "nn::lstm_cell" => Some(include_str!("../../stdlib/nn/lstm_cell.tern")),
        "nn::multihead_attn" => Some(include_str!("../../stdlib/nn/multihead_attn.tern")),
        "nn::optim::adagrad" => Some(include_str!("../../stdlib/nn/optim/adagrad.tern")),
        "nn::optim::adamw" => Some(include_str!("../../stdlib/nn/optim/adamw.tern")),
        "nn::optim::gradient_clip" => Some(include_str!("../../stdlib/nn/optim/gradient_clip.tern")),
        "nn::optim::lr_scheduler" => Some(include_str!("../../stdlib/nn/optim/lr_scheduler.tern")),
        "nn::optim::rmsprop" => Some(include_str!("../../stdlib/nn/optim/rmsprop.tern")),
        "nn::pooling" => Some(include_str!("../../stdlib/nn/pooling.tern")),
        "nn::positional_encoding" => Some(include_str!("../../stdlib/nn/positional_encoding.tern")),
        "nn::residual" => Some(include_str!("../../stdlib/nn/residual.tern")),
        "nn::train::augmentation" => Some(include_str!("../../stdlib/nn/train/augmentation.tern")),
        "nn::train::checkpointing" => Some(include_str!("../../stdlib/nn/train/checkpointing.tern")),
        "nn::train::data_loader" => Some(include_str!("../../stdlib/nn/train/data_loader.tern")),
        "nn::train::early_stopping" => Some(include_str!("../../stdlib/nn/train/early_stopping.tern")),
        "nn::train::profiler" => Some(include_str!("../../stdlib/nn/train/profiler.tern")),
        "nn::train::regularization" => Some(include_str!("../../stdlib/nn/train/regularization.tern")),
        "qnn::entanglement" => Some(include_str!("../../stdlib/qnn/entanglement.tern")),
        "qnn::qnn_circuit" => Some(include_str!("../../stdlib/qnn/qnn_circuit.tern")),
        "qnn::qnn_layer" => Some(include_str!("../../stdlib/qnn/qnn_layer.tern")),
        "qnn::qutrit" => Some(include_str!("../../stdlib/qnn/qutrit.tern")),
        "qnn::variational" => Some(include_str!("../../stdlib/qnn/variational.tern")),
        "reasoning::bayesian" => Some(include_str!("../../stdlib/reasoning/bayesian.tern")),
        "reasoning::causal" => Some(include_str!("../../stdlib/reasoning/causal.tern")),
        "reasoning::contradiction" => Some(include_str!("../../stdlib/reasoning/contradiction.tern")),
        "reasoning::temporal" => Some(include_str!("../../stdlib/reasoning/temporal.tern")),
        "reasoning::uncertainty" => Some(include_str!("../../stdlib/reasoning/uncertainty.tern")),
        "research::constitutional_trit" => Some(include_str!("../../stdlib/research/constitutional_trit.tern")),
        "research::moe_routing" => Some(include_str!("../../stdlib/research/moe_routing.tern")),
        "research::sparse_moe" => Some(include_str!("../../stdlib/research/sparse_moe.tern")),
        "research::ternary_ppo" => Some(include_str!("../../stdlib/research/ternary_ppo.tern")),
        "research::ternary_ssm" => Some(include_str!("../../stdlib/research/ternary_ssm.tern")),
        "research::trit_attention" => Some(include_str!("../../stdlib/research/trit_attention.tern")),
        "research::trit_diffusion" => Some(include_str!("../../stdlib/research/trit_diffusion.tern")),
        "research::trit_kv_cache" => Some(include_str!("../../stdlib/research/trit_kv_cache.tern")),
        "research::trit_memory" => Some(include_str!("../../stdlib/research/trit_memory.tern")),
        "research::trit_pruning" => Some(include_str!("../../stdlib/research/trit_pruning.tern")),
        "research::trit_quantization" => Some(include_str!("../../stdlib/research/trit_quantization.tern")),
        "research::tritformer" => Some(include_str!("../../stdlib/research/tritformer.tern")),
        "rl::actor_critic" => Some(include_str!("../../stdlib/rl/actor_critic.tern")),
        "rl::environment" => Some(include_str!("../../stdlib/rl/environment.tern")),
        "rl::episode" => Some(include_str!("../../stdlib/rl/episode.tern")),
        "rl::exploration" => Some(include_str!("../../stdlib/rl/exploration.tern")),
        "rl::policy" => Some(include_str!("../../stdlib/rl/policy.tern")),
        "rl::q_learning" => Some(include_str!("../../stdlib/rl/q_learning.tern")),
        "rl::reward_shaping" => Some(include_str!("../../stdlib/rl/reward_shaping.tern")),
        "rl::value_fn" => Some(include_str!("../../stdlib/rl/value_fn.tern")),
        "safety::adversarial" => Some(include_str!("../../stdlib/safety/adversarial.tern")),
        "safety::alignment_check" => Some(include_str!("../../stdlib/safety/alignment_check.tern")),
        "safety::calibration" => Some(include_str!("../../stdlib/safety/calibration.tern")),
        "safety::conformal" => Some(include_str!("../../stdlib/safety/conformal.tern")),
        "safety::content_gate" => Some(include_str!("../../stdlib/safety/content_gate.tern")),
        "safety::fairness" => Some(include_str!("../../stdlib/safety/fairness.tern")),
        "safety::ood_detect" => Some(include_str!("../../stdlib/safety/ood_detect.tern")),
        "safety::uncertainty_quant" => Some(include_str!("../../stdlib/safety/uncertainty_quant.tern")),
        "stats::bayesian_inference" => Some(include_str!("../../stdlib/stats/bayesian_inference.tern")),
        "stats::confidence_interval" => Some(include_str!("../../stdlib/stats/confidence_interval.tern")),
        "stats::correlation" => Some(include_str!("../../stdlib/stats/correlation.tern")),
        "stats::distributions" => Some(include_str!("../../stdlib/stats/distributions.tern")),
        "stats::hypothesis_test" => Some(include_str!("../../stdlib/stats/hypothesis_test.tern")),
        "stats::information_theory" => Some(include_str!("../../stdlib/stats/information_theory.tern")),
        "stats::regression_stats" => Some(include_str!("../../stdlib/stats/regression_stats.tern")),
        "std::collections" => Some(include_str!("../../stdlib/std/collections.tern")),
        "std::graph" => Some(include_str!("../../stdlib/std/graph.tern")),
        "std::io" => Some(include_str!("../../stdlib/std/io.tern")),
        "std::logic" => Some(include_str!("../../stdlib/std/logic.tern")),
        "std::math" => Some(include_str!("../../stdlib/std/math.tern")),
        "std::memory" => Some(include_str!("../../stdlib/std/memory.tern")),
        "std::signal" => Some(include_str!("../../stdlib/std/signal.tern")),
        "std::tensor" => Some(include_str!("../../stdlib/std/tensor.tern")),
        "std::trit" => Some(include_str!("../../stdlib/std/trit.tern")),
        "ternary::arithmetic" => Some(include_str!("../../stdlib/ternary/arithmetic.tern")),
        "ternary::consensus" => Some(include_str!("../../stdlib/ternary/consensus.tern")),
        "ternary::encoding" => Some(include_str!("../../stdlib/ternary/encoding.tern")),
        "ternary::gates" => Some(include_str!("../../stdlib/ternary/gates.tern")),
        "testing::assert" => Some(include_str!("../../stdlib/testing/assert.tern")),
        "testing::mock" => Some(include_str!("../../stdlib/testing/mock.tern")),
        "testing::suite" => Some(include_str!("../../stdlib/testing/suite.tern")),
        "timeseries::anomaly_ts" => Some(include_str!("../../stdlib/timeseries/anomaly_ts.tern")),
        "timeseries::changepoint" => Some(include_str!("../../stdlib/timeseries/changepoint.tern")),
        "timeseries::decomposition" => Some(include_str!("../../stdlib/timeseries/decomposition.tern")),
        "timeseries::features" => Some(include_str!("../../stdlib/timeseries/features.tern")),
        "timeseries::forecasting" => Some(include_str!("../../stdlib/timeseries/forecasting.tern")),
        "timeseries::lstm_forecast" => Some(include_str!("../../stdlib/timeseries/lstm_forecast.tern")),
        "timeseries::tcn" => Some(include_str!("../../stdlib/timeseries/tcn.tern")),
        "tutorials::01_hello_ternary" => Some(include_str!("../../stdlib/tutorials/01_hello_ternary.tern")),
        "tutorials::02_match_exhaustive" => Some(include_str!("../../stdlib/tutorials/02_match_exhaustive.tern")),
        "tutorials::03_tensors" => Some(include_str!("../../stdlib/tutorials/03_tensors.tern")),
        "tutorials::04_sparse_ops" => Some(include_str!("../../stdlib/tutorials/04_sparse_ops.tern")),
        "tutorials::05_functions" => Some(include_str!("../../stdlib/tutorials/05_functions.tern")),
        "tutorials::06_structs" => Some(include_str!("../../stdlib/tutorials/06_structs.tern")),
        "tutorials::07_agents" => Some(include_str!("../../stdlib/tutorials/07_agents.tern")),
        "tutorials::08_multi_agent" => Some(include_str!("../../stdlib/tutorials/08_multi_agent.tern")),
        "tutorials::09_ml_basics" => Some(include_str!("../../stdlib/tutorials/09_ml_basics.tern")),
        "tutorials::10_attention" => Some(include_str!("../../stdlib/tutorials/10_attention.tern")),
        "tutorials::11_training_loop" => Some(include_str!("../../stdlib/tutorials/11_training_loop.tern")),
        "tutorials::12_uncertainty" => Some(include_str!("../../stdlib/tutorials/12_uncertainty.tern")),
        "tutorials::13_consensus" => Some(include_str!("../../stdlib/tutorials/13_consensus.tern")),
        "tutorials::14_full_pipeline" => Some(include_str!("../../stdlib/tutorials/14_full_pipeline.tern")),
        "tutorials::15_ternary_philosophy" => Some(include_str!("../../stdlib/tutorials/15_ternary_philosophy.tern")),
        "vision::augmentation_2d" => Some(include_str!("../../stdlib/vision/augmentation_2d.tern")),
        "vision::feature_extractor" => Some(include_str!("../../stdlib/vision/feature_extractor.tern")),
        "vision::image_classifier" => Some(include_str!("../../stdlib/vision/image_classifier.tern")),
        "vision::image_ops" => Some(include_str!("../../stdlib/vision/image_ops.tern")),
        "vision::object_gate" => Some(include_str!("../../stdlib/vision/object_gate.tern")),
        "vision::segmentation_gate" => Some(include_str!("../../stdlib/vision/segmentation_gate.tern")),

        _ => None,
    }
}

// ─── Shared helpers ──────────────────────────────────────────────────────────

/// Recursively collect `use` paths from a slice of statements.
fn collect_use_paths(stmts: &[Stmt]) -> Vec<Vec<String>> {
    let mut paths = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::Use { path } => paths.push(path.clone()),
            Stmt::Block(inner) => paths.extend(collect_use_paths(inner)),
            Stmt::IfTernary { on_pos, on_zero, on_neg, .. } => {
                paths.extend(collect_use_paths(&[*on_pos.clone()]));
                paths.extend(collect_use_paths(&[*on_zero.clone()]));
                paths.extend(collect_use_paths(&[*on_neg.clone()]));
            }
            Stmt::Match { arms, .. } => {
                for (_, arm_stmt) in arms {
                    paths.extend(collect_use_paths(&[arm_stmt.clone()]));
                }
            }
            _ => {}
        }
    }
    paths
}

/// Parse source string and extract its functions; deduplicate against `known`.
fn parse_and_extract(src: &str, key: &str, known: &mut std::collections::HashSet<String>) -> Vec<Function> {
    let mut parser = Parser::new(src);
    match parser.parse_program() {
        Ok(prog) => prog.functions.into_iter().filter(|f| known.insert(f.name.clone())).collect(),
        Err(e)   => { eprintln!("[MOD-000] Failed to parse module '{key}': {e}"); vec![] }
    }
}

/// Resolve all `use` paths, injecting matching functions into `program`.
/// `extra_source` is called for paths not found in the stdlib — returns `Option<String>`.
fn resolve_with<F>(program: &mut Program, extra_source: F)
where
    F: Fn(&[String]) -> Option<String>,
{
    let mut known: std::collections::HashSet<String> =
        program.functions.iter().map(|f| f.name.clone()).collect();

    let mut all_paths: Vec<Vec<String>> = program
        .functions
        .iter()
        .flat_map(|f| collect_use_paths(&f.body))
        .collect();
    all_paths.sort();
    all_paths.dedup();

    let mut injected: Vec<Function> = Vec::new();

    for path in &all_paths {
        let key = path.join("::");
        if let Some(src) = stdlib_source_for(path) {
            injected.extend(parse_and_extract(src, &key, &mut known));
        } else if let Some(src) = extra_source(path) {
            injected.extend(parse_and_extract(&src, &key, &mut known));
        } else {
            eprintln!("[MOD-001] Unknown module '{key}' — no stdlib match and no file found. Did you mean std::trit?");
        }
    }

    injected.extend(program.functions.drain(..));
    program.functions = injected;
}

// ─── StdlibLoader (backwards-compat, stdlib-only) ────────────────────────────

pub struct StdlibLoader;

impl StdlibLoader {
    /// Resolve stdlib `use` statements in `program`.  User-defined modules are
    /// left for a `ModuleResolver` to handle.
    pub fn resolve(program: &mut Program) {
        resolve_with(program, |_| None);
    }
}

// ─── ModuleResolver (stdlib + user-defined cross-file modules) ───────────────

/// Full module resolver.  Resolves stdlib built-ins AND user `.tern` modules
/// found relative to a source file's directory.
///
/// # Usage
/// ```ignore
/// let mut resolver = ModuleResolver::from_source_file(Path::new("src/main.tern"));
/// resolver.resolve(&mut program);
/// ```
pub struct ModuleResolver {
    base_dir: Option<std::path::PathBuf>,
}

impl ModuleResolver {
    /// Resolve relative to the directory containing `source_file`.
    pub fn from_source_file(source_file: &std::path::Path) -> Self {
        Self { base_dir: source_file.parent().map(|p| p.to_path_buf()) }
    }

    /// Resolve relative to `dir` (directory, not file).
    pub fn from_dir(dir: std::path::PathBuf) -> Self {
        Self { base_dir: Some(dir) }
    }

    /// Stdlib-only resolver (no file-system access). Equivalent to `StdlibLoader`.
    pub fn stdlib_only() -> Self {
        Self { base_dir: None }
    }

    /// Attempt to load `path` (e.g. `["mymod", "utils"]`) as `base_dir/mymod/utils.tern`.
    fn load_user_module(&self, path: &[String]) -> Option<String> {
        let base = self.base_dir.as_ref()?;
        let mut file_path = base.clone();
        for (i, segment) in path.iter().enumerate() {
            if i == path.len() - 1 {
                file_path = file_path.join(format!("{segment}.tern"));
            } else {
                file_path = file_path.join(segment);
            }
        }
        match std::fs::read_to_string(&file_path) {
            Ok(src) => Some(src),
            Err(_)  => None,
        }
    }

    /// Resolve all `use` statements: stdlib first, then user files.
    pub fn resolve(&self, program: &mut Program) {
        resolve_with(program, |path| self.load_user_module(path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    /// Verify that each stdlib module parses without errors.
    #[test]
    fn all_stdlib_modules_parse() {
        let modules = [
            vec!["std".to_string(), "trit".to_string()],
            vec!["std".to_string(), "math".to_string()],
            vec!["std".to_string(), "tensor".to_string()],
            vec!["std".to_string(), "io".to_string()],
            vec!["ml".to_string(), "quantize".to_string()],
            vec!["ml".to_string(), "inference".to_string()],
            vec!["classical".to_string(), "svm".to_string()],
            vec!["nn".to_string(), "dense".to_string()],
            vec!["models".to_string(), "gpt_analog".to_string()],
            vec!["research".to_string(), "tritformer".to_string()],
        ];
        for path in &modules {
            let src = stdlib_source_for(path)
                .unwrap_or_else(|| panic!("Missing stdlib source for {}", path.join("::")));
            let mut parser = Parser::new(src);
            parser.parse_program()
                .unwrap_or_else(|e| panic!("Parse error in {}: {:?}", path.join("::"), e));
        }
    }

    /// A program with `use std::trit;` should gain abs/min/max/etc after resolve.
    #[test]
    fn resolve_injects_trit_stdlib() {
        let src = r#"
fn main() -> trit {
    use std::trit;
    let x: trit = abs(-1);
    return x;
}
"#;
        let mut parser = Parser::new(src);
        let mut prog = parser.parse_program().expect("parse failed");
        assert!(!prog.functions.iter().any(|f| f.name == "abs"),
            "abs should not be present before resolve");
        StdlibLoader::resolve(&mut prog);
        assert!(prog.functions.iter().any(|f| f.name == "abs"),
            "abs should be injected after resolve");
        assert!(prog.functions.iter().any(|f| f.name == "min"));
        assert!(prog.functions.iter().any(|f| f.name == "majority"));
    }

    /// Multiple use statements should all be resolved, with no duplicates.
    #[test]
    fn resolve_multiple_modules_no_duplicates() {
        let src = r#"
fn main() -> trit {
    use std::trit;
    use std::math;
    let x: trit = neg(1);
    return x;
}
"#;
        let mut parser = Parser::new(src);
        let mut prog = parser.parse_program().expect("parse failed");
        StdlibLoader::resolve(&mut prog);

        // Count how many times "neg" appears — should be exactly 1
        let neg_count = prog.functions.iter().filter(|f| f.name == "neg").count();
        assert_eq!(neg_count, 1, "neg should appear exactly once");

        // Both modules should be present
        assert!(prog.functions.iter().any(|f| f.name == "abs"));   // std::trit
        assert!(prog.functions.iter().any(|f| f.name == "rectify")); // std::math
    }

    /// Resolve is idempotent — calling it twice should not duplicate functions.
    #[test]
    fn resolve_is_idempotent() {
        let src = r#"
fn main() -> trit {
    use std::trit;
    return 0;
}
"#;
        let mut parser = Parser::new(src);
        let mut prog = parser.parse_program().expect("parse failed");
        StdlibLoader::resolve(&mut prog);
        StdlibLoader::resolve(&mut prog);
        let abs_count = prog.functions.iter().filter(|f| f.name == "abs").count();
        assert_eq!(abs_count, 1, "abs should not be duplicated by double resolve");
    }

    /// Unknown module paths are silently skipped (not a hard error).
    #[test]
    fn unknown_module_skipped_gracefully() {
        let src = r#"
fn main() -> trit {
    use std::nonexistent;
    return 0;
}
"#;
        let mut parser = Parser::new(src);
        let mut prog = parser.parse_program().expect("parse failed");
        // Should not panic
        StdlibLoader::resolve(&mut prog);
    }
}
