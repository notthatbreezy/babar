# WorkflowContext

Work Title: Typed SQL Unification
Work ID: typed-sql-unification
Base Branch: main
Target Branch: feature/typed-sql-unification
Execution Mode: worktree
Repository Identity: github.com/notthatbreezy/babar@0810d71a7fb7be949ac3d18273b3fb33892c8fe1
Execution Binding: worktree:typed-sql-unification:feature/typed-sql-unification
Workflow Mode: full
Review Strategy: local
Review Policy: milestones
Session Policy: continuous
Final Agent Review: enabled
Final Review Mode: single-model
Final Review Interactive: smart
Final Review Models: gpt-5.4
Final Review Specialists: all
Final Review Interaction Mode: parallel
Final Review Specialist Models: none
Final Review Perspectives: auto
Final Review Perspective Cap: 2
Implementation Model: none
Plan Generation Mode: single-model
Plan Generation Models: gpt-5.4
Planning Docs Review: enabled
Planning Review Mode: multi-model
Planning Review Interactive: smart
Planning Review Models: gpt-5.4, claude-opus-4.7, claude-sonnet-4.6
Planning Review Specialists: all
Planning Review Interaction Mode: parallel
Planning Review Specialist Models: none
Planning Review Perspectives: auto
Planning Review Perspective Cap: 2
Custom Workflow Instructions: none
Initial Prompt: Scope and implement the next stage of babar's typed SQL direction so typed_query becomes the primary typed SQL API. In scope for this round: expand typed_query runtime lowering to cover more of the authored schema type surface, add live verification so typed_query can subsume query!'s verification niche, and expand statement coverage beyond SELECT to a practical first write subset (at minimum INSERT/UPDATE/DELETE and RETURNING where appropriate). Keep simple_query_raw for migrations and other raw control-plane SQL. Focus first on scoping and feasibility: identify what limited SQL subset we should support initially, what raw escape hatches remain intentional, and whether typed_query should split into typed_query plus typed_command or evolve into one broader statement surface without painting us into a corner later.
Issue URL: none
Remote: origin
Artifact Lifecycle: commit-and-clean
Artifact Paths: auto-derived
Additional Inputs: none
