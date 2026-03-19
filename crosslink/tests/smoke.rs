mod smoke {
    pub mod harness;

    // CLI tests
    mod cli_data; // import/export, archive, knowledge
    mod cli_extended; // daemon, timer, session lifecycle, issue search/next/tested
    mod cli_infra; // config, sync, migrate, integrity, compact, prune
    mod cli_tooling; // cpitd, workflow, context, style, design_doc, mc

    // Server tests
    mod server_api; // REST endpoints + WebSocket
    mod server_extended; // comments, labels, blockers, subissues, filters

    // Coordination tests
    mod coordination; // events, compaction, locks, push retry, v1->v2
    mod shared_writer; // SharedWriter integration: multi-agent ops, offline, lock protocol

    // Adversarial tests
    mod adversarial; // boundary, corruption, injection, concurrency
    mod concurrency; // concurrent API, parallel lock claims, network-partition/offline

    // Lifecycle smoke tests
    mod lifecycle; // kickoff/swarm/daemon/timer lifecycle flows

    // TUI + proptest
    mod tui_proptest; // TUI renders, proptest extensions
}
