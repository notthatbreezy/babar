# WorkflowContext

Work Title: Mdbook Docs Rewrite
Work ID: mdbook-docs-rewrite
Base Branch: main
Target Branch: feature/mdbook-docs-rewrite
Execution Mode: current-checkout
Repository Identity: github.com/notthatbreezy/babar@0810d71a7fb7be949ac3d18273b3fb33892c8fe1
Execution Binding: none
Workflow Mode: full
Review Strategy: local
Review Policy: milestones
Session Policy: continuous
Final Agent Review: enabled
Final Review Mode: multi-model
Final Review Interactive: smart
Final Review Models: gpt-5.4, claude-opus-4.7
Final Review Specialists: all
Final Review Interaction Mode: parallel
Final Review Specialist Models: none
Final Review Perspectives: auto
Final Review Perspective Cap: 2
Implementation Model: none
Plan Generation Mode: single-model
Plan Generation Models: claude-sonnet-4.6
Planning Docs Review: enabled
Planning Review Mode: single-model
Planning Review Interactive: smart
Planning Review Models: claude-sonnet-4.6
Planning Review Specialists: all
Planning Review Interaction Mode: parallel
Planning Review Specialist Models: none
Planning Review Perspectives: auto
Planning Review Perspective Cap: 2
Custom Workflow Instructions: Documentation-only work. Do NOT modify Cargo.toml or any code under crates/. Use crates/core/examples/* as the source of truth for code samples. Run `mdbook build` at the end of each implementation phase. American English throughout. Voice target: typelevel doobie's "Book of Doobie" — conversational, code-first, second person, inline `// type: T` annotations, numbered chapters, each chapter self-contained with imports/setup at top.
Initial Prompt: Reorganize and rewrite the babar mdbook documentation site under docs/ in a Diataxis-aligned structure with doobie-style voice. Move SITE-COPY.md and landing-mockup.html out of docs/ to .design/. Rename and relocate brand images from images/ into docs/assets/images/ with kebab-case names. Author full prose for new chapters (book/how-to, reference, explanation). Keep existing 1162-line tutorial verbatim.
Issue URL: none
Remote: origin
Artifact Lifecycle: commit-and-clean
Artifact Paths: auto-derived
Additional Inputs: none
