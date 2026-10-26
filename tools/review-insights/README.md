# Review insights

Research into what reviewers keep pointing out on application-services PRs, and which of it could be caught by
linters, formatters or CI checks instead.

- **[PROPOSAL.md](PROPOSAL.md)**: findings and the proposed rollout. Start here.
- **[RULES.md](RULES.md)**: all 91 rules with evidence (review quotes, bugfix PRs) and what each finds today.
- `prototype/`: proposed `clippy.toml` and `[workspace.lints]`, 8 ast-grep rules and `checks.py`.
- `results/`: everything the report is built from (see below).
- `scripts/`, `prompts/`: how it was produced.

Nothing here is wired into the build. It is a proposal.

## Layout

```
prototype/
  clippy.toml                proposed top-level clippy.toml (validated with the pinned toolchain)
  workspace-lints.toml       proposed [workspace.lints], with today's hit count per lint
  clippy-measure-flags.txt   the -W flags used to measure results/clippy_counts.md
  sgconfig.yml, rules/       ast-grep rules
  checks.py                  repo checks that don't fit clippy/ast-grep (UniFFI names, UDL docs, headers, SQL, ...)
results/
  candidates/                raw LLM extraction output: rules_chunk_{1..7}.json (reviews), fixes_{1,2}.json (bugfix PRs)
  rules.json                 the 91 merged rules with counts and evidence
  threads_index.jsonl        url/date for every review thread cited as evidence
  clippy_counts.md           hits per candidate lint on main
  backtest.json              diff-based gates replayed on PRs merged since 2025
  tag_stats.md               keyword tags per year over all reviewer threads since 2018
scripts/
  fetch.py                   download PRs, review comments, review summaries, PR files (REST only)
  prepare.py                 build reviewer threads, keyword tags, and reading chunks
  select_prs.py              pick bugfix PRs / recent merged PRs for fetch.py --files
  render_fixes.py            render bugfix PR diffs into reading chunks
  canon.py                   the merge of 485 candidates into 91 rules, plus today's measurements
  consolidate.py             apply canon.py to the candidates -> results/rules.json
  verify_quotes.py           check every evidence quote against the source comment
  backtest.py                replay the diff-based gates on merged PRs
  render_report.py           results/rules.json -> RULES.md (and the tables in PROPOSAL.md)
  list_rules.py              list all candidates grouped by area (used while merging)
prompts/                     the two prompts given to the LLM subagents
```

## Reproduce

The raw download is ~60 MB, so it isn't checked in. You need `gh`/a GitHub token in `GH_TOKEN` (about 7,000 REST calls,
within one hour of the authenticated rate limit).

```sh
cd tools/review-insights
W=/tmp/review-insights; mkdir -p $W

# 1. Download everything (comments, review summaries, PR metadata).
python3 scripts/fetch.py $W/data --reviews

# 2. Threads, keyword stats, and 7 reading chunks of the threads since 2022.
python3 scripts/prepare.py $W/data $W/work --since 2022-01-01 --chunks 7

# 3. Bugfix PR diffs.
python3 scripts/fetch.py $W/data --skip-base --files "$(python3 scripts/select_prs.py $W/data/prs.jsonl fixes --since 2022)"
python3 scripts/render_fixes.py $W/data $W/work --chunks 2

# 4. LLM extraction: run prompts/extract_rules.md on each $W/work/chunk_N.txt and prompts/assess_bugfixes.md on each
#    $W/work/fixes_N.txt, writing into results/candidates/. (This is the only non-deterministic step.)
python3 scripts/verify_quotes.py $W/work/threads.jsonl results/candidates

# 5. Merge and render.
python3 scripts/consolidate.py results/candidates results/threads_index.jsonl results/rules.json
python3 scripts/render_report.py results/rules.json results/threads_index.jsonl .

# 6. Backtest the diff-based gates on PRs merged since 2025.
mkdir -p $W/backtest && cp $W/data/prs.jsonl $W/backtest/
python3 scripts/fetch.py $W/backtest --skip-base --files "$(python3 scripts/select_prs.py $W/data/prs.jsonl merged --since 2025-01-01)"
python3 scripts/backtest.py $W/backtest --examples
```

`render_report.py` also writes `summary_tables.md`, which holds the tables pasted into PROPOSAL.md.

### Running the prototype checks

```sh
# From the repository root.
python3 tools/review-insights/prototype/checks.py            # add --all-hits for every location
ast-grep scan -c tools/review-insights/prototype/sgconfig.yml components   # pip install ast-grep-cli

# Clippy with the candidate lints, library code only. nss_sys only needs headers for `cargo clippy`:
# point NSS_DIR at a directory whose include/nss/ holds the NSS and NSPR headers (e.g. from libnss3-dev).
NSS_DIR=... CLIPPY_CONF_DIR=tools/review-insights/prototype \
  cargo clippy --workspace --lib -- $(cat tools/review-insights/prototype/clippy-measure-flags.txt)
```
