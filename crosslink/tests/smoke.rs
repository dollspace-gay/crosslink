mod smoke {
    pub mod harness;

    // CLI tests
    mod cli_data; // import/export, archive, knowledge
    mod cli_extended; // daemon, timer, session, issue search/next/tested, close-all, agent, migrate
    mod cli_infra; // config, sync, migrate, integrity, compact, prune
    mod cli_tooling; // cpitd, workflow, context, style, design_doc, mc
    mod lifecycle; // timer roundtrip, session lifecycle, intervene, issue tree, daemon/swarm/kickoff

    // Server tests
    mod server_api; // REST endpoints + WebSocket
    mod server_extended; // comments, labels, blockers, subissues, usage tracking

    // Coordination tests
    mod coordination; // events, compaction, locks, push retry, v1->v2
    mod shared_writer; // SharedWriter integration: multi-agent issue ops, offline promotion

    // Adversarial tests
    mod adversarial; // boundary, corruption, injection, concurrency

    // Concurrency and network-partition tests
    mod concurrency;

    // TUI + proptest
    mod tui_proptest; // TUI renders, proptest extensions
}
