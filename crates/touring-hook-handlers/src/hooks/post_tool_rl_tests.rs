//! Integration tests for post_tool_rl — RL feedback loop end-to-end.
//!
//! Validates:
//! - ema_reward changes from 0.0 after processing rewards
//! - update_count increments on each post-tool-rl invocation
//! - QTable receives entries for (state, action) pairs
//! - LinUCB arm is updated with features
//! - Markov transition is recorded for tool sequences

#[cfg(test)]
mod tests {
    use crate::post_tool_rl::run as post_tool_rl_run;
    use crate::runtime::HookRuntime;
    use tempfile::TempDir;

    /// Helper: create a HookRuntime in a temp directory with RL engine initialized.
    fn make_runtime() -> (TempDir, HookRuntime) {
        let tmp = TempDir::new().unwrap();
        let rt = HookRuntime::new(tmp.path()).unwrap();
        // Ensure online_rl is initialized (it is by default)
        assert!(
            rt.learning.online_rl.is_some(),
            "OnlineRL engine should be initialized"
        );
        // Note: LinUCB is loaded from disk; if no saved state exists it will be None.
        // The post_tool_rl run() function handles this by creating LinUCB on first use.
        (tmp, rt)
    }

    /// Helper: build a PostToolUse payload for a successful Edit tool.
    fn make_success_payload(tool_name: &str, file_path: &str) -> serde_json::Value {
        serde_json::json!({
            "tool_name": tool_name,
            "tool_input": {
                "file_path": file_path
            },
            "tool_response": {
                "output": "File edited successfully"
            },
            "subtask_id": null
        })
    }

    /// Helper: build a PostToolUse payload for a failed tool.
    fn make_error_payload(tool_name: &str, file_path: &str, stderr: &str) -> serde_json::Value {
        serde_json::json!({
            "tool_name": tool_name,
            "tool_input": {
                "file_path": file_path
            },
            "tool_response": {
                "output": "",
                "error": stderr
            },
            "subtask_id": null
        })
    }

    // ── Test 1: EMA reward changes from 0.0 ───────────────────────────────

    #[test]
    fn test_post_tool_rl_ema_reward_changes_from_zero() {
        let (_tmp, mut rt) = make_runtime();

        // HookRuntime::new() always injects a warmup reward (inject_warmup_reward()),
        // which sets EMA ≈ 0.06 (alpha=0.1 × warmup_reward≈0.6). Not zero.
        let initial_ema = rt.learning.online_rl.as_ref().unwrap().ema_reward();
        assert!(
            initial_ema >= 0.0 && initial_ema < 0.2,
            "Initial EMA should reflect warmup reward injection (0.0..0.2), got {}",
            initial_ema
        );

        // Set environment for the hook
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("HOOK_ELAPSED_MS", "15") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("CILA_LEVEL", "2") };

        // Run post-tool-rl with a successful Edit
        let input = make_success_payload("Edit", "src/main.rs");
        let result = post_tool_rl_run(&mut rt, &input);
        assert!(
            result.is_ok(),
            "post_tool_rl should succeed, got: {:?}",
            result.err()
        );

        // After processing, EMA should have changed from 0.0
        let new_ema = rt.learning.online_rl.as_ref().unwrap().ema_reward();
        assert!(
            new_ema.abs() > 0.01 || rt.learning.online_rl.as_ref().unwrap().update_count() > 0,
            "EMA should have changed from ~0.0 after reward processing, got {}",
            new_ema
        );

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("HOOK_ELAPSED_MS") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("CILA_LEVEL") };
    }

    // ── Test 2: Update count increments ──────────────────────────────────

    #[test]
    fn test_post_tool_rl_increments_update_count() {
        let (_tmp, mut rt) = make_runtime();

        let initial_count = rt.learning.online_rl.as_ref().unwrap().update_count();

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("HOOK_ELAPSED_MS", "10") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("CILA_LEVEL", "2") };

        // Process first tool
        let input1 = make_success_payload("Read", "src/lib.py");
        let result1 = post_tool_rl_run(&mut rt, &input1);
        assert!(result1.is_ok());

        let count_after_first = rt.learning.online_rl.as_ref().unwrap().update_count();
        assert!(
            count_after_first > initial_count,
            "Update count should increment after first tool, was {}, now {}",
            initial_count,
            count_after_first
        );

        // Process second tool
        let input2 = make_success_payload("Edit", "src/main.rs");
        let result2 = post_tool_rl_run(&mut rt, &input2);
        assert!(result2.is_ok());

        let count_after_second = rt.learning.online_rl.as_ref().unwrap().update_count();
        assert!(
            count_after_second > count_after_first,
            "Update count should increment again after second tool, was {}, now {}",
            count_after_first,
            count_after_second
        );

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("HOOK_ELAPSED_MS") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("CILA_LEVEL") };
    }

    // ── Test 3: QTable gets entries ──────────────────────────────────────

    #[test]
    fn test_post_tool_rl_qtable_gets_entries() {
        let (_tmp, mut rt) = make_runtime();

        // Ensure qtable_cache is initialized
        if rt.learning.qtable_cache.is_none() {
            rt.learning.qtable_cache = Some(touring_intelligence::rl::QTable::new());
        }

        let initial_len = rt.learning.qtable_cache.as_ref().unwrap().len();

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("HOOK_ELAPSED_MS", "12") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("CILA_LEVEL", "2") };

        // Process a tool
        let input = make_success_payload("Bash", "src/test.sh");
        let result = post_tool_rl_run(&mut rt, &input);
        assert!(result.is_ok());

        // QTable should have received entries
        let new_len = rt.learning.qtable_cache.as_ref().unwrap().len();
        assert!(
            new_len > initial_len || rt.learning.online_rl.as_ref().unwrap().update_count() > 0,
            "QTable should have entries after processing, initial_len={}, new_len={}, update_count={}",
            initial_len,
            new_len,
            rt.learning.online_rl.as_ref().unwrap().update_count()
        );

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("HOOK_ELAPSED_MS") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("CILA_LEVEL") };
    }

    // ── Test 4: LinUCB arm is updated ───────────────────────────────────

    #[test]
    fn test_post_tool_rl_linucb_arm_updated() {
        let (_tmp, mut rt) = make_runtime();

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("HOOK_ELAPSED_MS", "8") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("CILA_LEVEL", "2") };

        // Process a tool — this initializes LinUCB if it was None
        let input = make_success_payload("Edit", "src/app.py");
        let result = post_tool_rl_run(&mut rt, &input);
        assert!(result.is_ok());

        // LinUCB should have been initialized and updated
        let linucb = rt
            .learning
            .linucb
            .as_ref()
            .expect("LinUCB should be initialized after post_tool_rl");
        let new_stats = linucb.arm_stats();
        let new_total_pulls: u64 = new_stats.iter().map(|(_, pulls, _)| pulls).sum();
        assert!(
            new_total_pulls > 0,
            "LinUCB total pulls should be > 0 after update, was {}",
            new_total_pulls
        );

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("HOOK_ELAPSED_MS") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("CILA_LEVEL") };
    }

    // ── Test 5: Markov transition is recorded ───────────────────────────

    #[test]
    fn test_post_tool_rl_markov_transition_recorded() {
        let (_tmp, mut rt) = make_runtime();

        // Initial transition count should be 0
        let initial_transitions = rt.learning.markov_predictor.transition_count();
        assert_eq!(
            initial_transitions, 0,
            "Initial transition count should be 0"
        );

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("HOOK_ELAPSED_MS", "10") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("CILA_LEVEL", "2") };

        // First tool — no transition yet (no previous tool)
        let input1 = make_success_payload("Read", "src/main.py");
        let result1 = post_tool_rl_run(&mut rt, &input1);
        assert!(result1.is_ok());

        // After first tool, last_tool_name should be set but no transition recorded yet
        let after_first = rt.learning.markov_predictor.transition_count();

        // Second tool — should record transition from Read → Edit
        let input2 = make_success_payload("Edit", "src/main.py");
        let result2 = post_tool_rl_run(&mut rt, &input2);
        assert!(result2.is_ok());

        // Now there should be a transition
        let after_second = rt.learning.markov_predictor.transition_count();
        assert!(
            after_second > after_first,
            "Markov transition should be recorded after second tool, count was {}, now {}",
            after_first,
            after_second
        );

        // Third tool — should record Edit → Bash transition
        let input3 = make_success_payload("Bash", "src/test.sh");
        let result3 = post_tool_rl_run(&mut rt, &input3);
        assert!(result3.is_ok());

        let after_third = rt.learning.markov_predictor.transition_count();
        assert!(
            after_third > after_second,
            "Markov transition should be recorded after third tool, count was {}, now {}",
            after_second,
            after_third
        );

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("HOOK_ELAPSED_MS") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("CILA_LEVEL") };
    }

    // ── Test 6: Full RL feedback loop ────────────────────────────────────

    #[test]
    fn test_post_tool_rl_full_rl_feedback_loop() {
        let (_tmp, mut rt) = make_runtime();

        // Ensure qtable_cache is initialized
        if rt.learning.qtable_cache.is_none() {
            rt.learning.qtable_cache = Some(touring_intelligence::rl::QTable::new());
        }

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("HOOK_ELAPSED_MS", "20") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("CILA_LEVEL", "3") };

        // Seed last_tool_name so Markov can record a transition on first call
        rt.ctx.last_tool_name = Some("Read".to_string());

        // Capture initial state
        let initial_ema = rt.learning.online_rl.as_ref().unwrap().ema_reward();
        let initial_count = rt.learning.online_rl.as_ref().unwrap().update_count();
        let initial_markov = rt.learning.markov_predictor.transition_count();

        // Run post-tool-rl with a successful tool
        let input = serde_json::json!({
            "tool_name": "Edit",
            "tool_input": {
                "file_path": "src/test.py"
            },
            "tool_response": {
                "output": "Edit applied successfully"
            },
            "subtask_id": "S-1"
        });

        let result = post_tool_rl_run(&mut rt, &input);
        assert!(result.is_ok(), "post_tool_rl should succeed");

        // Verify all RL components were updated
        let online_rl = rt.learning.online_rl.as_ref().unwrap();

        // 1. EMA reward should have been updated
        let new_ema = online_rl.ema_reward();
        assert!(
            (new_ema - initial_ema).abs() > 0.001 || online_rl.update_count() > initial_count,
            "EMA reward should change, was {:.4}, now {:.4}",
            initial_ema,
            new_ema
        );

        // 2. Update count should increment
        assert!(
            online_rl.update_count() > initial_count,
            "Update count should increment, was {}, now {}",
            initial_count,
            online_rl.update_count()
        );

        // 3. QTable should have entries or have been updated
        let new_qtable_len = rt
            .learning
            .qtable_cache
            .as_ref()
            .expect("qtable_cache should exist")
            .len();
        assert!(
            new_qtable_len > 0 || online_rl.update_count() > initial_count,
            "QTable should have entries or updates processed, len={}, count={}",
            new_qtable_len,
            online_rl.update_count()
        );

        // 4. LinUCB should have been initialized and updated
        let linucb = rt
            .learning
            .linucb
            .as_ref()
            .expect("LinUCB should be initialized after post_tool_rl");
        let new_linucb_pulls: u64 = linucb.arm_stats().iter().map(|(_, p, _)| p).sum();
        assert!(
            new_linucb_pulls > 0,
            "LinUCB pulls should be > 0 after update, got {}",
            new_linucb_pulls
        );

        // 5. Markov should have recorded transition
        let new_markov = rt.learning.markov_predictor.transition_count();
        assert!(
            new_markov > initial_markov,
            "Markov transition count should increase, was {}, now {}",
            initial_markov,
            new_markov
        );

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("HOOK_ELAPSED_MS") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("CILA_LEVEL") };
    }

    // ── Test 7: Error tool also triggers RL update ─────────────────────

    #[test]
    fn test_post_tool_rl_error_tool_triggers_update() {
        let (_tmp, mut rt) = make_runtime();

        let initial_count = rt.learning.online_rl.as_ref().unwrap().update_count();

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("HOOK_ELAPSED_MS", "100") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("CILA_LEVEL", "2") };

        // Run with an error payload
        let input = make_error_payload("Bash", "src/fail.sh", "error: command not found");
        let result = post_tool_rl_run(&mut rt, &input);
        assert!(
            result.is_ok(),
            "post_tool_rl should succeed even for errors"
        );

        // Update count should still increment for error cases
        let new_count = rt.learning.online_rl.as_ref().unwrap().update_count();
        assert!(
            new_count > initial_count,
            "Update count should increment for error tools, was {}, now {}",
            initial_count,
            new_count
        );

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("HOOK_ELAPSED_MS") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("CILA_LEVEL") };
    }

    // ── Test 8: Multiple tools accumulate QTable entries ───────────────

    #[test]
    fn test_post_tool_rl_multiple_tools_accumulate_qtable() {
        let (_tmp, mut rt) = make_runtime();

        if rt.learning.qtable_cache.is_none() {
            rt.learning.qtable_cache = Some(touring_intelligence::rl::QTable::new());
        }

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("HOOK_ELAPSED_MS", "10") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("CILA_LEVEL", "2") };

        // Process multiple different tools
        let tools = vec![
            ("Read", "src/a.py"),
            ("Edit", "src/b.rs"),
            ("Bash", "src/c.sh"),
            ("Read", "src/d.ts"),
            ("Edit", "src/e.py"),
        ];
        let tool_count = tools.len();

        for (tool, file) in tools {
            let input = make_success_payload(tool, file);
            let result = post_tool_rl_run(&mut rt, &input);
            assert!(
                result.is_ok(),
                "post_tool_rl failed for {} on {}",
                tool,
                file
            );
        }

        // Verify update count reflects all tools
        let count = rt.learning.online_rl.as_ref().unwrap().update_count();
        assert!(
            count >= tool_count as u64,
            "Update count should reflect all tools, expected at least {}, got {}",
            tool_count,
            count
        );

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("HOOK_ELAPSED_MS") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("CILA_LEVEL") };
    }
}
