
CREATE TABLE IF NOT EXISTS schema_migrations(
  version TEXT PRIMARY KEY, applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);

CREATE TABLE IF NOT EXISTS projects(
  id TEXT PRIMARY KEY, root_path TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('claude_code','codex')),
  display_name TEXT, created_at TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS sessions(
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id),
  file_path TEXT NOT NULL, started_at TEXT, last_seq INTEGER NOT NULL DEFAULT 0,
  raw_line_count INTEGER NOT NULL DEFAULT 0,
  raw_digest TEXT,
  head_chain_hash TEXT,
  event_count INTEGER NOT NULL DEFAULT 0);

CREATE INDEX IF NOT EXISTS idx_sessions_project ON sessions(project_id);

-- L9.6: the project's chain position, maintained at append so the read is one
-- row rather than a fold over every event. Derived state only — `rebuild_heads`
-- recomputes it from `events` and the result must be byte-identical.
CREATE TABLE IF NOT EXISTS project_heads(
  project_id TEXT PRIMARY KEY,
  digest TEXT NOT NULL,
  event_total INTEGER NOT NULL,
  computed_at TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS events(
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL, session_id TEXT NOT NULL,
  seq INTEGER NOT NULL,
  ts TEXT,
  kind TEXT NOT NULL CHECK(kind IN
    ('user_msg','assistant_text','assistant_thinking','tool_use','tool_result','summary','meta')),
  role TEXT, text TEXT,
  tool_name TEXT, source_tool_name TEXT, tool_use_id TEXT, tool_input TEXT, tool_result TEXT,
  is_error INTEGER NOT NULL DEFAULT 0,
  src_uuid TEXT, parent_uuid TEXT, is_sidechain INTEGER NOT NULL DEFAULT 0,
  source_turn_id TEXT, source_turn_boundary_line INTEGER,
  source_turn_boundary_sha256 TEXT, source_message_key TEXT,
  line_session_id TEXT,
  raw_file TEXT NOT NULL, raw_line INTEGER NOT NULL, raw_sha256 TEXT NOT NULL,
  prev_hash TEXT NOT NULL, chain_hash TEXT NOT NULL,
  UNIQUE(session_id, seq));
CREATE INDEX IF NOT EXISTS idx_events_project_ts ON events(project_id, ts);
CREATE INDEX IF NOT EXISTS idx_events_tool_use ON events(tool_use_id);
CREATE INDEX IF NOT EXISTS idx_events_session_line ON events(session_id, raw_line);
CREATE INDEX IF NOT EXISTS idx_events_src_uuid ON events(project_id, src_uuid);
CREATE TRIGGER IF NOT EXISTS events_no_update BEFORE UPDATE ON events
  BEGIN SELECT RAISE(ABORT,'events are append-only'); END;
CREATE TRIGGER IF NOT EXISTS events_no_delete BEFORE DELETE ON events
  BEGIN SELECT RAISE(ABORT,'events are append-only'); END;

CREATE TABLE IF NOT EXISTS annotations(
  id TEXT PRIMARY KEY, event_id TEXT NOT NULL REFERENCES events(id),
  layer TEXT NOT NULL,
  model TEXT NOT NULL, prompt_sha256 TEXT NOT NULL,
  output_json TEXT NOT NULL, output_sha256 TEXT NOT NULL,
  confidence REAL, created_at TEXT NOT NULL);
-- §5t: classification annotations are addressed by their complete input
-- identity. A model swap or prompt change appends; an identical input reuses
-- the first immutable row.
CREATE UNIQUE INDEX IF NOT EXISTS idx_classification_annotation_key
  ON annotations(event_id, model, prompt_sha256)
  WHERE layer = 'classification';
-- `INSERT OR REPLACE` performs a hidden delete before inserting and SQLite
-- does not run delete triggers for that path unless recursive triggers are
-- enabled. Guard the insert itself so REPLACE cannot rewrite history through
-- either the annotation id or the classification input key.
CREATE TRIGGER IF NOT EXISTS classification_annotations_no_replace
  BEFORE INSERT ON annotations
  WHEN EXISTS(
         SELECT 1 FROM annotations old
         WHERE old.id = NEW.id AND old.layer = 'classification'
       )
    OR (NEW.layer = 'classification' AND EXISTS(
         SELECT 1 FROM annotations old
         WHERE old.layer = 'classification'
           AND old.event_id = NEW.event_id
           AND old.model = NEW.model
           AND old.prompt_sha256 = NEW.prompt_sha256
       ))
  BEGIN SELECT RAISE(ABORT,'classification annotations are append-only'); END;
CREATE TRIGGER IF NOT EXISTS classification_annotations_no_update
  BEFORE UPDATE ON annotations
  WHEN OLD.layer = 'classification' OR NEW.layer = 'classification'
  BEGIN SELECT RAISE(ABORT,'classification annotations are append-only'); END;
CREATE TRIGGER IF NOT EXISTS classification_annotations_no_delete
  BEFORE DELETE ON annotations
  WHEN OLD.layer = 'classification'
  BEGIN SELECT RAISE(ABORT,'classification annotations are append-only'); END;

-- §7 plus three amendments recorded in the brief (M3.4):
--   * `kind`/`marker` on claims — a completed-work claim and a proposal take
--     different passes and different verdicts, so the class cannot live only
--     in the extractor's head.
--   * `fidelity`/`pass` on evidence_links — M3.5's verdict logic turns on
--     whether the premise was understood, so the tag must survive to the
--     receipt.
--   * `scorer` joins the evidence_links primary key, so a changed model
--     appends a new row instead of colliding with the old one.
CREATE TABLE IF NOT EXISTS claims(
  id TEXT PRIMARY KEY, event_id TEXT NOT NULL REFERENCES events(id),
  text TEXT NOT NULL, span_start INTEGER, span_end INTEGER,
  extractor TEXT NOT NULL, confidence REAL,
  -- M3.8 §5b: the scope principle enforced by the database rather than by
  -- convention. `work` and `marker` are the only accusable kinds. An extractor
  -- that invents a sixth category fails at INSERT rather than quietly widening
  -- what the product is willing to accuse people of.
  kind TEXT NOT NULL CHECK(kind IN ('work','marker','finding','micro_intent','proposal')),
  marker TEXT);
CREATE INDEX IF NOT EXISTS idx_claims_event ON claims(event_id);

-- M4.2: mutable head of the rebuildable claim projection. Classification
-- outputs and completed run manifests stay append-only; this pointer records
-- which complete projection is currently authoritative, including the safe
-- Tier-0 fallback and an all-demoted (zero-claim) language run.
CREATE TABLE IF NOT EXISTS claim_projection_heads(
  project_id TEXT PRIMARY KEY,
  extractor TEXT NOT NULL,
  manifest_annotation_id TEXT REFERENCES annotations(id)
);

-- M4.4: one explicit head names the complete entailment projection used by
-- derivation and export. Timestamps are audit metadata, not run ordering.
CREATE TABLE IF NOT EXISTS entailment_projection_heads(
  project_id TEXT PRIMARY KEY,
  scorer TEXT NOT NULL
);

-- M4 P4I: an accepted event advances the project's immutable source frontier,
-- so any optional language or scorer projection completed against the old
-- frontier becomes inert in the same SQLite statement. Historical
-- annotations, claims, evidence, and manifests remain append-only audit rows;
-- only their rebuildable authority pointers are cleared. An idempotent ingest
-- that inserts no event cannot fire this trigger.
CREATE TRIGGER IF NOT EXISTS events_invalidate_optional_projection_heads
  AFTER INSERT ON events
  BEGIN
    UPDATE claim_projection_heads
       SET manifest_annotation_id = NULL
     WHERE project_id = NEW.project_id
       AND manifest_annotation_id IS NOT NULL;
    UPDATE entailment_projection_heads
       SET scorer = ''
     WHERE project_id = NEW.project_id
       AND scorer <> '';
  END;

CREATE TABLE IF NOT EXISTS evidence_links(
  claim_id TEXT NOT NULL REFERENCES claims(id),
  evidence_event_id TEXT NOT NULL REFERENCES events(id),
  entail_score REAL, verdict TEXT NOT NULL,
  scorer TEXT NOT NULL,
  fidelity TEXT NOT NULL DEFAULT 'fallback',
  pass TEXT NOT NULL DEFAULT 'backward',
  contradiction REAL,
  related INTEGER NOT NULL DEFAULT 0,
  -- Typed contract v0 (A33): what the recorded run actually reported, frozen
  -- at annotate time from the same parser the premise renderer uses, so the
  -- contract and the prose can never disagree about the numbers.
  ev_family TEXT NOT NULL DEFAULT 'none',
  ev_passed INTEGER,
  ev_failed INTEGER,
  ev_ok INTEGER,
  -- A32: how strongly this evidence is ABOUT the claim, frozen at annotate
  -- time. Derivation ranks on it and accuses only at the top of the ranking,
  -- so the receipt can always name which single pair a warning rested on.
  match_strength REAL NOT NULL DEFAULT 0,
  -- §5i: the NORMALISED invocation this evidence represents, computed at
  -- annotate time (where the per-line `cwd` from raw JSONL is reachable) and
  -- frozen. Derivation groups supersession on this and never re-derives it, so
  -- the identity a verdict rested on stays reconstructible from the ledger.
  ev_identity TEXT NOT NULL DEFAULT '',
  -- M4.4 proof scope v0. Session/seq are copied from immutable events; cwd and
  -- git branch are admitted from the raw line only after its hash matches the
  -- hash frozen at ingest. NULL means the scope dimension was not knowable.
  ev_session_id TEXT,
  ev_seq INTEGER,
  ev_cwd TEXT,
  ev_git_branch TEXT,
  PRIMARY KEY(claim_id, evidence_event_id, scorer));

-- A34: proof that a search happened, separate from what it found.
--
-- `dropped` is an accusation — it says a thread was abandoned. Its evidence is
-- an ABSENCE, and an absence is only meaningful if someone actually looked.
-- Without this table a claim with no evidence_links is indistinguishable from
-- a claim `annotate` never reached, so a skipped run, a locked database or an
-- unusable search term all render as "nobody did this". `name_matches` is the
-- cheap positive signal that defeats the accusation: later actions naming the
-- target do not prove fulfilment, but they do disprove abandonment.
CREATE TABLE IF NOT EXISTS claim_searches(
  claim_id TEXT NOT NULL REFERENCES claims(id),
  scorer TEXT NOT NULL,
  pass TEXT NOT NULL,
  searched INTEGER NOT NULL,
  name_matches INTEGER NOT NULL,
  -- §5j defeater tiers (A52): a name match defeats `dropped` only from
  -- action-side text (tool_input — commands, edit paths, written content).
  -- Output-only mentions annotate the receipt and never defeat alone.
  action_matches INTEGER NOT NULL DEFAULT 0,
  output_matches INTEGER NOT NULL DEFAULT 0,
  -- §5r: an `unsupported` verdict is an assertion about scan coverage.
  -- Freeze whether the dominant evidence families in this claim's local
  -- neighborhood were read completely, plus the families that were not.
  coverage_eligible INTEGER NOT NULL DEFAULT 0,
  missing_families TEXT NOT NULL DEFAULT '',
  PRIMARY KEY(claim_id, scorer, pass));

CREATE TABLE IF NOT EXISTS nodes(
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL, label TEXT NOT NULL,
  -- M4.1: the old flat state is retained only as a deterministic sort key.
  -- Lifecycle, evidence, and currency are independent facts; no caller may
  -- recover one dimension by guessing from another.
  sort_key TEXT NOT NULL CHECK(sort_key IN
    ('recorded','supported','pending','unknown','unsupported','dropped','contradicted','refused')),
  lifecycle TEXT NOT NULL CHECK(lifecycle IN
    ('proposed','accepted','active','completed','abandoned')),
  evidence_state TEXT NOT NULL CHECK(evidence_state IN
    ('unknown','observed','supported','verified','contradicted','stale')),
  currency TEXT NOT NULL CHECK(currency IN
    ('current','superseded','out_of_scope')),
  confidence REAL, session_id TEXT NOT NULL,
  anchor_event_id TEXT NOT NULL REFERENCES events(id),
  derived_rev INTEGER NOT NULL);
-- A191 (P2a): every per-project read of the projection was a FULL TABLE SCAN.
-- `projection_nodes` selects `WHERE project_id = ?1 ORDER BY id`, and with no
-- index on `project_id` SQLite planned `SCAN nodes USING INDEX
-- sqlite_autoindex_nodes_1` — walking all 87,132 rows and using the primary-key
-- autoindex only to satisfy the ORDER BY. `packet` compiles one such read per
-- project in scope, so a repo-scoped packet cost O(projects × TOTAL ledger
-- nodes): a 10-event project cost exactly as much as a 165k-event one, because
-- the scan is of the whole table either way (measured A190).
--
-- The index is on `project_id` alone rather than `(project_id, id)`: the
-- surgical change is the one the diagnosis named, and a covering index is a
-- second thing whose effect could not be isolated in the same measurement.
CREATE INDEX IF NOT EXISTS idx_nodes_project ON nodes(project_id);
CREATE TABLE IF NOT EXISTS edges(
  from_node TEXT NOT NULL, to_node TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('seq','inferred')),
  derived_rev INTEGER NOT NULL, PRIMARY KEY(from_node,to_node,kind));

-- §5u: dismissal is an append-only acknowledgment record. Attention state is
-- never mutated and history is never rewritten to make the past look calmer,
-- so this table carries the same update/delete refusal as the M5 lifecycle
-- records. Item ids are content-addressed, so a dismissal expires naturally
-- when its subject changes rather than needing a sweep.
CREATE TABLE IF NOT EXISTS attention_acknowledgments(
  item_id TEXT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('dismissed')),
  acknowledged_at_unix_ms INTEGER NOT NULL,
  PRIMARY KEY(item_id, action, acknowledged_at_unix_ms));
CREATE INDEX IF NOT EXISTS idx_attention_ack_item ON attention_acknowledgments(item_id);

-- A187: the adjudication store. Where a HUMAN's judgment on a shadow-class
-- hypothesis is recorded — who, when, and by what route. Append-only under the
-- same triggers as the acknowledgments above, and for the same reason: a
-- product that can edit its own record of what a person decided has stopped
-- being a ledger. `specimen_id` is content-addressed over the specimen, so a
-- verdict cannot silently outlive a change in the thing it judged.
CREATE TABLE IF NOT EXISTS adjudications(
  specimen_id TEXT NOT NULL,
  class TEXT NOT NULL,
  subject TEXT NOT NULL,
  verdict TEXT NOT NULL CHECK(verdict IN ('agree','disagree','insufficient_evidence')),
  operator TEXT NOT NULL,
  route TEXT NOT NULL,
  adjudicated_at_unix_ms INTEGER NOT NULL,
  PRIMARY KEY(specimen_id, adjudicated_at_unix_ms));
CREATE INDEX IF NOT EXISTS idx_adjudications_class ON adjudications(class);
CREATE TRIGGER IF NOT EXISTS adjudications_no_update
  BEFORE UPDATE ON adjudications
  BEGIN SELECT RAISE(ABORT,'adjudications are append-only'); END;
CREATE TRIGGER IF NOT EXISTS adjudications_no_delete
  BEFORE DELETE ON adjudications
  BEGIN SELECT RAISE(ABORT,'adjudications are append-only'); END;

-- M4 P3A: entity association is a navigation instrument, never an identity
-- merge. The proposal binds two exact event receipts in one project; the
-- human decision is a separate one-shot row. Neither table is consulted by
-- derivation, and neither can update projects, sessions, events, chains,
-- nodes, or source identities.
CREATE TABLE IF NOT EXISTS entity_association_proposals(
  candidate_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id),
  first_event_id TEXT NOT NULL REFERENCES events(id),
  first_span_start INTEGER,
  first_span_end INTEGER,
  second_event_id TEXT NOT NULL REFERENCES events(id),
  second_span_start INTEGER,
  second_span_end INTEGER,
  label TEXT NOT NULL,
  proposer TEXT NOT NULL CHECK(proposer IN ('operator','model')),
  proposer_operator TEXT,
  annotation_id TEXT REFERENCES annotations(id),
  proposed_at_unix_ms INTEGER NOT NULL CHECK(proposed_at_unix_ms >= 0),
  CHECK(first_event_id < second_event_id),
  CHECK((first_span_start IS NULL AND first_span_end IS NULL) OR
        (first_span_start IS NOT NULL AND first_span_end IS NOT NULL AND
         first_span_start >= 0 AND first_span_end > first_span_start)),
  CHECK((second_span_start IS NULL AND second_span_end IS NULL) OR
        (second_span_start IS NOT NULL AND second_span_end IS NOT NULL AND
         second_span_start >= 0 AND second_span_end > second_span_start)),
  CHECK((proposer='operator' AND annotation_id IS NULL AND
         proposer_operator IS NOT NULL AND length(trim(proposer_operator)) > 0) OR
        (proposer='model' AND annotation_id IS NOT NULL AND proposer_operator IS NULL)));
CREATE INDEX IF NOT EXISTS idx_entity_association_project
  ON entity_association_proposals(project_id, proposed_at_unix_ms, candidate_id);
CREATE TRIGGER IF NOT EXISTS entity_association_proposals_exact_receipts
  BEFORE INSERT ON entity_association_proposals
  WHEN NOT EXISTS(
         SELECT 1 FROM events e
          WHERE e.id=NEW.first_event_id AND e.project_id=NEW.project_id)
    OR NOT EXISTS(
         SELECT 1 FROM events e
          WHERE e.id=NEW.second_event_id AND e.project_id=NEW.project_id)
  BEGIN SELECT RAISE(ABORT,'entity association receipts must share the exact project'); END;
CREATE TRIGGER IF NOT EXISTS entity_association_model_annotation
  BEFORE INSERT ON entity_association_proposals
  WHEN NEW.proposer='model' AND NOT EXISTS(
         SELECT 1 FROM annotations a
          WHERE a.id=NEW.annotation_id AND a.layer='classification'
            AND a.event_id IN (NEW.first_event_id, NEW.second_event_id))
  BEGIN SELECT RAISE(ABORT,'entity association model proposal needs a subject classification annotation'); END;
CREATE TRIGGER IF NOT EXISTS entity_association_proposals_no_replace
  BEFORE INSERT ON entity_association_proposals
  WHEN EXISTS(
         SELECT 1 FROM entity_association_proposals old
          WHERE old.candidate_id=NEW.candidate_id)
  BEGIN SELECT RAISE(ABORT,'entity association proposals are append-only'); END;
CREATE TRIGGER IF NOT EXISTS entity_association_proposals_no_update
  BEFORE UPDATE ON entity_association_proposals
  BEGIN SELECT RAISE(ABORT,'entity association proposals are append-only'); END;
CREATE TRIGGER IF NOT EXISTS entity_association_proposals_no_delete
  BEFORE DELETE ON entity_association_proposals
  BEGIN SELECT RAISE(ABORT,'entity association proposals are append-only'); END;

CREATE TABLE IF NOT EXISTS entity_association_decisions(
  candidate_id TEXT PRIMARY KEY REFERENCES entity_association_proposals(candidate_id),
  decision TEXT NOT NULL CHECK(decision IN
    ('associate','reject','insufficient_evidence')),
  operator TEXT NOT NULL CHECK(length(trim(operator)) > 0),
  route TEXT NOT NULL CHECK(length(trim(route)) > 0),
  decided_at_unix_ms INTEGER NOT NULL CHECK(decided_at_unix_ms >= 0));
CREATE TRIGGER IF NOT EXISTS entity_association_decisions_no_replace
  BEFORE INSERT ON entity_association_decisions
  WHEN EXISTS(
         SELECT 1 FROM entity_association_decisions old
          WHERE old.candidate_id=NEW.candidate_id)
  BEGIN SELECT RAISE(ABORT,'entity association decisions are one-shot and append-only'); END;
CREATE TRIGGER IF NOT EXISTS entity_association_decisions_no_update
  BEFORE UPDATE ON entity_association_decisions
  BEGIN SELECT RAISE(ABORT,'entity association decisions are append-only'); END;
CREATE TRIGGER IF NOT EXISTS entity_association_decisions_no_delete
  BEFORE DELETE ON entity_association_decisions
  BEGIN SELECT RAISE(ABORT,'entity association decisions are append-only'); END;
CREATE TRIGGER IF NOT EXISTS attention_acknowledgments_no_update
  BEFORE UPDATE ON attention_acknowledgments
  BEGIN SELECT RAISE(ABORT,'attention acknowledgments are append-only'); END;
CREATE TRIGGER IF NOT EXISTS attention_acknowledgments_no_delete
  BEFORE DELETE ON attention_acknowledgments
  BEGIN SELECT RAISE(ABORT,'attention acknowledgments are append-only'); END;

CREATE TABLE IF NOT EXISTS ledger_meta(
  project_id TEXT PRIMARY KEY, head_hash TEXT NOT NULL,
  event_count INTEGER NOT NULL, verified_at TEXT,
  -- A143-era `verified` conflated two facts; A150 splits them (P1). NULL means
  -- "never measured", never "fine": chain_intact is the chain's own arithmetic,
  -- raw_files_missing is dead receipts — a vendor-retention property, not a
  -- wound. The UI must never again say "broken" about an intact chain.
  chain_intact INTEGER, raw_files_missing INTEGER);

-- Each ledger owns a disjoint private-mirror namespace. A mirror that is not
-- referenced by this database is not thereby unaccepted by another database;
-- the persistent authority key makes that distinction mechanical.
CREATE TABLE IF NOT EXISTS ledger_authority(
  singleton INTEGER PRIMARY KEY CHECK(singleton=1),
  authority_key TEXT NOT NULL UNIQUE);
CREATE TRIGGER IF NOT EXISTS ledger_authority_no_update BEFORE UPDATE ON ledger_authority
  BEGIN SELECT RAISE(ABORT,'ledger authority identity is immutable'); END;
CREATE TRIGGER IF NOT EXISTS ledger_authority_no_delete BEFORE DELETE ON ledger_authority
  BEGIN SELECT RAISE(ABORT,'ledger authority identity is append-only'); END;

-- M4.R2/A71: source identity is not project presentation metadata. Existing
-- Claude ids remain the authority; new source projects are scoped by owner,
-- source kind, and source-native identity. Normalized roots are hints only.
CREATE TABLE IF NOT EXISTS source_project_identities(
  project_id TEXT PRIMARY KEY REFERENCES projects(id),
  owner_key TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('claude_code','codex')),
  source_project_key TEXT NOT NULL,
  normalized_root_hint TEXT,
  UNIQUE(owner_key, source, source_project_key));
CREATE INDEX IF NOT EXISTS idx_source_project_root_hint
  ON source_project_identities(normalized_root_hint);

-- R13 packet retention. Two tables on purpose.
--
-- `packet_contents` is CONTENT-ADDRESSED: two fetches at the same chain
-- position produce byte-identical packets by construction, so they share one
-- row. `packet_issues` is APPEND-ONLY and records every hand-out separately,
-- because "we served this twice" is a different fact from "this content
-- exists" and collapsing them would lose the audit.
--
-- Retention is what makes deltas possible at all: the projection is
-- current-only and derivation has no as-of-position mode, so the only way to
-- say what changed since position A is to have kept what was handed over at A.
-- The diff is therefore against a RECEIPT, not against a reconstruction of
-- what we believe we handed over.
CREATE TABLE IF NOT EXISTS packet_contents(
  content_sha256 TEXT PRIMARY KEY,
  lines_json TEXT NOT NULL,
  line_count INTEGER NOT NULL);

CREATE TABLE IF NOT EXISTS packet_issues(
  issue_id TEXT PRIMARY KEY,
  chain_position TEXT NOT NULL,
  scope TEXT NOT NULL,
  content_sha256 TEXT NOT NULL REFERENCES packet_contents(content_sha256),
  served_by_json TEXT NOT NULL,
  agent TEXT,
  issued_at_unix_ms INTEGER NOT NULL,
  issued_seq INTEGER NOT NULL UNIQUE,
  -- R17b-2b successor linkage: the issue this packet continues from. NULL means
  -- this is the start of a chain, not that the link was forgotten.
  predecessor_issue_id TEXT REFERENCES packet_issues(issue_id));
CREATE INDEX IF NOT EXISTS idx_packet_issues_position
  ON packet_issues(chain_position, issued_seq);
CREATE TRIGGER IF NOT EXISTS packet_issues_no_update BEFORE UPDATE ON packet_issues
  BEGIN SELECT RAISE(ABORT,'packet_issues are append-only'); END;
CREATE TRIGGER IF NOT EXISTS packet_issues_no_delete BEFORE DELETE ON packet_issues
  BEGIN SELECT RAISE(ABORT,'packet_issues are append-only'); END;

-- A94 projection head. Records THAT a project's derived projection was built,
-- and from which chain position.
--
-- Without it, an empty `nodes` result is ambiguous between "this project has
-- nothing needing a human" and "this project has never been read" — and the
-- attention queue resolved that ambiguity the wrong way, telling 43 of 44
-- projects "everything checks out" when nothing had been looked at. Absence of
-- a projection is not absence of a problem. Derived and rebuildable; carries no
-- authority over truth.
CREATE TABLE IF NOT EXISTS node_projection_heads(
  project_id TEXT PRIMARY KEY REFERENCES projects(id),
  head_hash TEXT NOT NULL,
  node_count INTEGER NOT NULL,
  built_at TEXT NOT NULL);

-- A89 MCP call record. An APPEND-ONLY SIBLING of the event ledger, in the M5
-- capture tradition: it never writes an event, never rewrites a chain, and has
-- no authority over truth.
--
-- Why a sibling and not an event: `Binding.digest` folds `compute_project_head`
-- over EVERY event in a project, so writing an event per call would move the
-- chain position the packet had just issued — `check_context` would answer
-- "not current" for a position the agent received milliseconds earlier, purely
-- because it asked. The gate would become noise. The call is still ledgered as
-- a real event with a real receipt, by the NORMAL path: the client writes its
-- own `mcp__*` tool_use/tool_result into the session transcript, and ingest
-- turns those into events. `event_id` is bound afterwards, never at call time —
-- at the moment the tool runs, the transcript line does not exist yet.
--
-- `outcome` deliberately CANNOT say done or completed: a submission is a
-- submission, and the verdict stays with the verifier.
CREATE TABLE IF NOT EXISTS mcp_calls(
  call_id TEXT PRIMARY KEY,
  agent TEXT NOT NULL,
  tool TEXT NOT NULL CHECK(tool IN ('get_context_packet','check_context','claim_completion')),
  scope TEXT NOT NULL,
  obligation TEXT,
  gated_chain_position TEXT,
  latest_chain_position TEXT,
  request_sha256 TEXT NOT NULL,
  manifest_json TEXT,
  manifest_sha256 TEXT,
  outcome TEXT NOT NULL CHECK(outcome IN
    ('served','current','not_current','ready_for_verification','rejected_stale_position','rejected_missing_fields')),
  accepted_at_unix_ms INTEGER NOT NULL,
  accepted_seq INTEGER NOT NULL UNIQUE,
  event_id TEXT REFERENCES events(id));
CREATE INDEX IF NOT EXISTS idx_mcp_calls_agent ON mcp_calls(agent, accepted_seq);
CREATE TRIGGER IF NOT EXISTS mcp_calls_no_update BEFORE UPDATE ON mcp_calls
  BEGIN SELECT RAISE(ABORT,'mcp_calls are append-only'); END;
CREATE TRIGGER IF NOT EXISTS mcp_calls_no_delete BEFORE DELETE ON mcp_calls
  BEGIN SELECT RAISE(ABORT,'mcp_calls are append-only'); END;

-- A126: which ingested transcript event a recorded MCP call turned out to be.
--
-- A SEPARATE TABLE, and that is the whole point. `mcp_calls.event_id` cannot
-- ever be filled: the row is written while the call is being served, the
-- transcript event does not exist until that session is later ingested, and the
-- triggers above forbid every UPDATE. So the binding is DERIVED state — same
-- rule as `nodes`, `project_heads` and `workspace_identities` — rebuildable
-- from `mcp_calls` + `events`, and never a mutation of the append-only record.
CREATE TABLE IF NOT EXISTS mcp_call_bindings(
  call_id TEXT PRIMARY KEY REFERENCES mcp_calls(call_id),
  event_id TEXT NOT NULL REFERENCES events(id),
  basis TEXT NOT NULL,
  bound_at TEXT NOT NULL,
  UNIQUE(event_id));

-- R04/A92 repo identity. DERIVED and ADDITIVE: `project_id` remains the chain
-- genesis and is never rewritten, projects are never merged, and nothing here
-- has authority over an event. A project whose working directory is gone gets
-- basis='unresolvable' with repo_id NULL — abstention is recorded, never
-- guessed, because the A88 census measured the stored root_path wrong for 75%
-- of the corpus. `resolved_via` and `sessions_sampled` are the provenance of
-- the resolution itself: cwd is not in the ledger and must be read from raw
-- transcripts, so a resolution that cannot say how it was reached is exactly
-- the unreceipted fact this project refuses. No triggers: this table is derived
-- and re-runnable, not a receipt.
CREATE TABLE IF NOT EXISTS workspace_identities(
  project_id TEXT PRIMARY KEY REFERENCES projects(id),
  repo_id TEXT,
  basis TEXT NOT NULL CHECK(basis IN ('git','marker','unresolvable')),
  root_commits_json TEXT NOT NULL DEFAULT '[]',
  remote_fingerprints_json TEXT NOT NULL DEFAULT '[]',
  resolved_root TEXT,
  resolved_via TEXT CHECK(resolved_via IN ('recorded_cwd','reconstructed')),
  sessions_sampled INTEGER NOT NULL DEFAULT 0,
  observed_at TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS idx_workspace_identities_repo
  ON workspace_identities(repo_id);

CREATE TABLE IF NOT EXISTS source_sessions(
  session_id TEXT PRIMARY KEY REFERENCES sessions(id),
  project_id TEXT NOT NULL REFERENCES projects(id),
  owner_key TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('claude_code','codex')),
  source_session_key TEXT NOT NULL,
  source_locator TEXT,
  UNIQUE(owner_key, source, source_session_key));

-- Presence in `session_generations` means accepted. Rejected candidates live
-- only in `candidate_assessments` and have zero truth authority.
CREATE TABLE IF NOT EXISTS session_generations(
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id),
  session_id TEXT NOT NULL REFERENCES sessions(id),
  predecessor_id TEXT REFERENCES session_generations(id),
  mirror_payload_path TEXT NOT NULL,
  mirror_seal_path TEXT NOT NULL,
  payload_sha256 TEXT NOT NULL,
  payload_bytes INTEGER NOT NULL,
  seal_sha256 TEXT NOT NULL,
  inherited_prefix_bytes INTEGER NOT NULL,
  total_envelopes INTEGER NOT NULL,
  local_envelopes INTEGER NOT NULL,
  local_events INTEGER NOT NULL,
  accepted_at TEXT NOT NULL,
  UNIQUE(session_id, payload_sha256));
CREATE TRIGGER IF NOT EXISTS session_generations_no_update BEFORE UPDATE ON session_generations
  BEGIN SELECT RAISE(ABORT,'accepted generations are immutable'); END;
CREATE TRIGGER IF NOT EXISTS session_generations_no_delete BEFORE DELETE ON session_generations
  BEGIN SELECT RAISE(ABORT,'accepted generations are append-only'); END;
CREATE TABLE IF NOT EXISTS session_generation_heads(
  session_id TEXT PRIMARY KEY REFERENCES sessions(id),
  generation_id TEXT NOT NULL UNIQUE REFERENCES session_generations(id));

CREATE TABLE IF NOT EXISTS generation_raw_dispositions(
  generation_id TEXT NOT NULL REFERENCES session_generations(id),
  raw_line INTEGER NOT NULL,
  raw_sha256 TEXT NOT NULL,
  byte_start INTEGER NOT NULL,
  byte_length INTEGER NOT NULL,
  envelope_family TEXT NOT NULL,
  disposition TEXT NOT NULL,
  reason TEXT,
  source_turn_id TEXT,
  turn_boundary_raw_line INTEGER,
  turn_boundary_raw_sha256 TEXT,
  emitted_event_count INTEGER NOT NULL,
  PRIMARY KEY(generation_id, raw_line));
CREATE TRIGGER IF NOT EXISTS generation_raw_dispositions_no_update
  BEFORE UPDATE ON generation_raw_dispositions
  BEGIN SELECT RAISE(ABORT,'generation dispositions are immutable'); END;
CREATE TRIGGER IF NOT EXISTS generation_raw_dispositions_no_delete
  BEFORE DELETE ON generation_raw_dispositions
  BEGIN SELECT RAISE(ABORT,'generation dispositions are append-only'); END;

CREATE TABLE IF NOT EXISTS event_generation_provenance(
  event_id TEXT PRIMARY KEY REFERENCES events(id),
  generation_id TEXT NOT NULL REFERENCES session_generations(id),
  raw_line INTEGER NOT NULL,
  FOREIGN KEY(generation_id, raw_line)
    REFERENCES generation_raw_dispositions(generation_id, raw_line));
-- A196: verify's per-generation accounting filters this table by generation_id,
-- whose only other index is the event_id primary key — so each COUNT was a full
-- scan (1.5M rows / 225 MB), run twice per generation. At 20,354 generations
-- that is ~9 TB of page reads; whole-ledger verify could not finish (measured
-- A196: 8.7 s per scan, scoped verify 71.9 s -> 57 ms with the index).
CREATE INDEX IF NOT EXISTS idx_generation_provenance_generation
  ON event_generation_provenance(generation_id);
CREATE TRIGGER IF NOT EXISTS event_generation_provenance_no_update
  BEFORE UPDATE ON event_generation_provenance
  BEGIN SELECT RAISE(ABORT,'event generation provenance is immutable'); END;
CREATE TRIGGER IF NOT EXISTS event_generation_provenance_no_delete
  BEFORE DELETE ON event_generation_provenance
  BEGIN SELECT RAISE(ABORT,'event generation provenance is append-only'); END;

-- M4.R2: authoritative response-message occurrences are durable one-use slots.
-- A fallback suppression is an append-only receipt, never an update to the
-- canonical event or slot. Both tables commit with their source generation.
CREATE TABLE IF NOT EXISTS codex_message_slots(
  slot_key TEXT PRIMARY KEY,
  event_id TEXT NOT NULL UNIQUE REFERENCES events(id),
  session_id TEXT NOT NULL REFERENCES sessions(id),
  generation_id TEXT NOT NULL REFERENCES session_generations(id),
  mirror_key TEXT NOT NULL,
  raw_line INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS idx_codex_message_slots_session_key
  ON codex_message_slots(session_id,mirror_key,raw_line,slot_key);
-- A196: same verify accounting path as idx_generation_provenance_generation.
CREATE INDEX IF NOT EXISTS idx_codex_message_slots_generation
  ON codex_message_slots(generation_id);
CREATE TRIGGER IF NOT EXISTS codex_message_slots_no_update BEFORE UPDATE ON codex_message_slots
  BEGIN SELECT RAISE(ABORT,'Codex message slots are immutable'); END;
CREATE TRIGGER IF NOT EXISTS codex_message_slots_no_delete BEFORE DELETE ON codex_message_slots
  BEGIN SELECT RAISE(ABORT,'Codex message slots are append-only'); END;
CREATE TABLE IF NOT EXISTS codex_reconciliation_receipts(
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id),
  generation_id TEXT NOT NULL REFERENCES session_generations(id),
  observation_raw_line INTEGER NOT NULL,
  observation_raw_sha256 TEXT NOT NULL,
  slot_key TEXT NOT NULL UNIQUE REFERENCES codex_message_slots(slot_key),
  match_basis TEXT NOT NULL,
  source_turn_id TEXT,
  turn_boundary_raw_line INTEGER,
  turn_boundary_raw_sha256 TEXT,
  FOREIGN KEY(generation_id, observation_raw_line)
    REFERENCES generation_raw_dispositions(generation_id, raw_line));
-- A196: same verify accounting path as idx_generation_provenance_generation.
CREATE INDEX IF NOT EXISTS idx_codex_reconciliation_generation
  ON codex_reconciliation_receipts(generation_id);
CREATE TRIGGER IF NOT EXISTS codex_reconciliation_no_update
  BEFORE UPDATE ON codex_reconciliation_receipts
  BEGIN SELECT RAISE(ABORT,'Codex reconciliation receipts are immutable'); END;
CREATE TRIGGER IF NOT EXISTS codex_reconciliation_no_delete
  BEFORE DELETE ON codex_reconciliation_receipts
  BEGIN SELECT RAISE(ABORT,'Codex reconciliation receipts are append-only'); END;

CREATE TABLE IF NOT EXISTS candidate_assessments(
  id TEXT PRIMARY KEY,
  owner_key TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('claude_code','codex')),
  source_project_key TEXT NOT NULL,
  source_session_key TEXT NOT NULL,
  predecessor_generation_id TEXT,
  candidate_sha256 TEXT,
  candidate_bytes INTEGER,
  raw_envelopes INTEGER,
  status TEXT NOT NULL CHECK(status IN
    ('successor_rejected','source_diverged','continuity_unknown','capture_failed','transaction_failed')),
  reason TEXT NOT NULL,
  observed_at TEXT NOT NULL);
CREATE TRIGGER IF NOT EXISTS candidate_assessments_no_update BEFORE UPDATE ON candidate_assessments
  BEGIN SELECT RAISE(ABORT,'candidate assessments are immutable'); END;
CREATE TRIGGER IF NOT EXISTS candidate_assessments_no_delete BEFORE DELETE ON candidate_assessments
  BEGIN SELECT RAISE(ABORT,'candidate assessments are append-only'); END;

-- M5.R1/A76: Assured Capture is an append-only evidence substrate beside the
-- event ledger.  It never rewrites events, annotations, verdicts, or chains.
CREATE TABLE IF NOT EXISTS collector_keys(
  key_id TEXT PRIMARY KEY,
  owner_key TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('claude_code','codex')),
  algorithm TEXT NOT NULL CHECK(algorithm='ed25519'),
  public_key_hex TEXT NOT NULL,
  created_at_unix_ms INTEGER NOT NULL,
  UNIQUE(owner_key,source,public_key_hex));
CREATE TRIGGER IF NOT EXISTS collector_keys_no_update BEFORE UPDATE ON collector_keys
  BEGIN SELECT RAISE(ABORT,'collector keys are immutable'); END;
CREATE TRIGGER IF NOT EXISTS collector_keys_no_delete BEFORE DELETE ON collector_keys
  BEGIN SELECT RAISE(ABORT,'collector keys use append-only revocation receipts'); END;

CREATE TABLE IF NOT EXISTS collector_key_receipts(
  id TEXT PRIMARY KEY,
  key_id TEXT NOT NULL REFERENCES collector_keys(key_id),
  action TEXT NOT NULL CHECK(action IN ('rotated','revoked')),
  reason TEXT NOT NULL,
  created_at_unix_ms INTEGER NOT NULL);
CREATE TRIGGER IF NOT EXISTS collector_key_receipts_no_update BEFORE UPDATE ON collector_key_receipts
  BEGIN SELECT RAISE(ABORT,'collector key receipts are immutable'); END;
CREATE TRIGGER IF NOT EXISTS collector_key_receipts_no_delete BEFORE DELETE ON collector_key_receipts
  BEGIN SELECT RAISE(ABORT,'collector key receipts are append-only'); END;

-- A source can enable `assured` only after its exact version/config has passed
-- the measured blocking/rewrite/identity/delivery gates.  No Claude row is
-- created by M5.R1 because its controlled timing boundary is still unproven.
CREATE TABLE IF NOT EXISTS source_assurance_attestations(
  id TEXT PRIMARY KEY,
  owner_key TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('claude_code','codex')),
  source_version TEXT NOT NULL,
  config_sha256 TEXT NOT NULL,
  matcher TEXT NOT NULL,
  pre_blocks INTEGER NOT NULL CHECK(pre_blocks IN (0,1)),
  rewrite_exact INTEGER NOT NULL CHECK(rewrite_exact IN (0,1)),
  stable_tool_identity INTEGER NOT NULL CHECK(stable_tool_identity IN (0,1)),
  completion_pairing INTEGER NOT NULL CHECK(completion_pairing IN (0,1)),
  measurement_json TEXT NOT NULL,
  measurement_sha256 TEXT NOT NULL,
  key_id TEXT NOT NULL REFERENCES collector_keys(key_id),
  signature_hex TEXT NOT NULL,
  accepted_at_unix_ms INTEGER NOT NULL,
  UNIQUE(owner_key,source,source_version,config_sha256,measurement_sha256));
CREATE TRIGGER IF NOT EXISTS source_assurance_no_update BEFORE UPDATE ON source_assurance_attestations
  BEGIN SELECT RAISE(ABORT,'source assurance attestations are immutable'); END;
CREATE TRIGGER IF NOT EXISTS source_assurance_no_delete BEFORE DELETE ON source_assurance_attestations
  BEGIN SELECT RAISE(ABORT,'source assurance attestations are append-only'); END;

CREATE TABLE IF NOT EXISTS capture_attempts(
  attempt_id TEXT PRIMARY KEY,
  owner_key TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('claude_code','codex')),
  source_project_key TEXT NOT NULL,
  source_session_key TEXT NOT NULL,
  source_turn_id TEXT,
  source_tool_id TEXT NOT NULL,
  wrapper_nonce TEXT NOT NULL,
  command_sha256 TEXT NOT NULL,
  source_config_sha256 TEXT NOT NULL,
  mechanism TEXT NOT NULL,
  source_version TEXT NOT NULL,
  profile TEXT NOT NULL,
  strength TEXT NOT NULL CHECK(strength IN
    ('assured','bounded_observation','historical_only','unknown')),
  status TEXT NOT NULL CHECK(status IN
    ('accepted','orphaned','rejected','quarantined')),
  failure_code TEXT,
  started_monotonic_ns INTEGER,
  ended_monotonic_ns INTEGER,
  exit_code INTEGER,
  term_signal INTEGER,
  receipt_sha256 TEXT,
  created_at_unix_ms INTEGER NOT NULL,
  UNIQUE(owner_key,source,source_project_key,source_session_key,
         source_tool_id,wrapper_nonce));
CREATE TRIGGER IF NOT EXISTS capture_attempts_no_update BEFORE UPDATE ON capture_attempts
  BEGIN SELECT RAISE(ABORT,'capture attempts are immutable'); END;
CREATE TRIGGER IF NOT EXISTS capture_attempts_no_delete BEFORE DELETE ON capture_attempts
  BEGIN SELECT RAISE(ABORT,'capture attempts are append-only'); END;

CREATE TABLE IF NOT EXISTS state_capsules(
  capsule_id TEXT PRIMARY KEY,
  attempt_id TEXT NOT NULL REFERENCES capture_attempts(attempt_id),
  phase TEXT NOT NULL CHECK(phase IN ('pre','post','observation')),
  schema_version INTEGER NOT NULL,
  prior_capsule_id TEXT,
  canonical_sha256 TEXT NOT NULL,
  signature_hex TEXT NOT NULL,
  key_id TEXT NOT NULL REFERENCES collector_keys(key_id),
  private_relative_ref TEXT NOT NULL,
  complete INTEGER NOT NULL CHECK(complete IN (0,1)),
  completeness_json TEXT NOT NULL,
  repository_id TEXT,
  common_dir_id TEXT,
  worktree_id TEXT,
  platform_id TEXT,
  head_ref_digest TEXT,
  detached INTEGER NOT NULL DEFAULT 0 CHECK(detached IN (0,1)),
  operation_state_digest TEXT,
  head_commit TEXT,
  head_tree TEXT,
  index_digest TEXT,
  tracked_worktree_digest TEXT,
  staged_digest TEXT,
  untracked_digest TEXT,
  toolchain_digest TEXT,
  lock_config_digest TEXT,
  environment_digest TEXT,
  profile_digest TEXT,
  accepted_seq INTEGER NOT NULL UNIQUE,
  accepted_at_unix_ms INTEGER NOT NULL,
  UNIQUE(attempt_id,phase));
CREATE INDEX IF NOT EXISTS idx_state_capsules_lineage
  ON state_capsules(repository_id,common_dir_id,worktree_id,accepted_seq);
CREATE TRIGGER IF NOT EXISTS state_capsules_no_update BEFORE UPDATE ON state_capsules
  BEGIN SELECT RAISE(ABORT,'state capsules are immutable'); END;
CREATE TRIGGER IF NOT EXISTS state_capsules_no_delete BEFORE DELETE ON state_capsules
  BEGIN SELECT RAISE(ABORT,'state capsules are append-only'); END;

CREATE TABLE IF NOT EXISTS capture_event_links(
  id TEXT PRIMARY KEY,
  attempt_id TEXT NOT NULL UNIQUE REFERENCES capture_attempts(attempt_id),
  event_id TEXT NOT NULL UNIQUE REFERENCES events(id),
  source_tool_id TEXT NOT NULL,
  delivery_sha256 TEXT NOT NULL,
  linked_at_unix_ms INTEGER NOT NULL);
CREATE TRIGGER IF NOT EXISTS capture_event_links_no_update BEFORE UPDATE ON capture_event_links
  BEGIN SELECT RAISE(ABORT,'capture event links are immutable'); END;
CREATE TRIGGER IF NOT EXISTS capture_event_links_no_delete BEFORE DELETE ON capture_event_links
  BEGIN SELECT RAISE(ABORT,'capture event links are append-only'); END;

CREATE TABLE IF NOT EXISTS state_comparisons(
  comparison_id TEXT PRIMARY KEY,
  execution_capsule_id TEXT NOT NULL REFERENCES state_capsules(capsule_id),
  observation_capsule_id TEXT NOT NULL REFERENCES state_capsules(capsule_id),
  currentness TEXT NOT NULL CHECK(currentness IN ('current','stale','unknown')),
  comparable_dimensions_json TEXT NOT NULL,
  changed_dimensions_json TEXT NOT NULL,
  reason TEXT NOT NULL,
  accepted_seq INTEGER NOT NULL UNIQUE,
  accepted_at_unix_ms INTEGER NOT NULL,
  UNIQUE(execution_capsule_id,observation_capsule_id));
CREATE TRIGGER IF NOT EXISTS state_comparisons_no_update BEFORE UPDATE ON state_comparisons
  BEGIN SELECT RAISE(ABORT,'state comparisons are immutable'); END;
CREATE TRIGGER IF NOT EXISTS state_comparisons_no_delete BEFORE DELETE ON state_comparisons
  BEGIN SELECT RAISE(ABORT,'state comparisons are append-only'); END;

CREATE TABLE IF NOT EXISTS capture_continuity_receipts(
  id TEXT PRIMARY KEY,
  owner_key TEXT NOT NULL,
  source TEXT NOT NULL,
  source_project_key TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN
    ('capture_failed','source_diverged','continuity_unknown')),
  reason TEXT NOT NULL,
  accepted_seq INTEGER NOT NULL UNIQUE,
  observed_at_unix_ms INTEGER NOT NULL);
CREATE TRIGGER IF NOT EXISTS capture_continuity_no_update BEFORE UPDATE ON capture_continuity_receipts
  BEGIN SELECT RAISE(ABORT,'capture continuity receipts are immutable'); END;
CREATE TRIGGER IF NOT EXISTS capture_continuity_no_delete BEFORE DELETE ON capture_continuity_receipts
  BEGIN SELECT RAISE(ABORT,'capture continuity receipts are append-only'); END;

-- M5.R2/A78: product activation is an append-only operational layer.  These
-- records may gate future capture authority; they never rewrite evidence,
-- annotations, verdicts, or an already accepted capture.
CREATE TABLE IF NOT EXISTS capture_install_plans(
  plan_id TEXT PRIMARY KEY,
  owner_key TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('claude_code','codex')),
  config_identity TEXT NOT NULL,
  config_preimage_sha256 TEXT NOT NULL,
  proposed_config_sha256 TEXT NOT NULL,
  source_version TEXT NOT NULL,
  source_executable_sha256 TEXT NOT NULL,
  wrapper_sha256 TEXT NOT NULL,
  matcher_json TEXT NOT NULL,
  proposed_config_json TEXT NOT NULL,
  plan_sha256 TEXT NOT NULL UNIQUE,
  created_at_unix_ms INTEGER NOT NULL);
CREATE TRIGGER IF NOT EXISTS capture_install_plans_no_update BEFORE UPDATE ON capture_install_plans
  BEGIN SELECT RAISE(ABORT,'capture install plans are immutable'); END;
CREATE TRIGGER IF NOT EXISTS capture_install_plans_no_delete BEFORE DELETE ON capture_install_plans
  BEGIN SELECT RAISE(ABORT,'capture install plans are append-only'); END;

CREATE TABLE IF NOT EXISTS capture_lifecycle_records(
  record_id TEXT PRIMARY KEY,
  owner_key TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('claude_code','codex')),
  action TEXT NOT NULL CHECK(action IN
    ('installed','enabled','disabled','uninstalled','recovered','observation_failed')),
  supersedes_record_id TEXT REFERENCES capture_lifecycle_records(record_id),
  plan_id TEXT REFERENCES capture_install_plans(plan_id),
  config_identity TEXT NOT NULL,
  config_sha256 TEXT,
  source_version TEXT,
  source_executable_sha256 TEXT,
  wrapper_sha256 TEXT,
  collector_key_id TEXT,
  detail_json TEXT NOT NULL,
  record_sha256 TEXT NOT NULL,
  key_id TEXT REFERENCES collector_keys(key_id),
  signature_hex TEXT,
  accepted_seq INTEGER NOT NULL UNIQUE,
  accepted_at_unix_ms INTEGER NOT NULL);
CREATE TRIGGER IF NOT EXISTS capture_lifecycle_no_update BEFORE UPDATE ON capture_lifecycle_records
  BEGIN SELECT RAISE(ABORT,'capture lifecycle records are immutable'); END;
CREATE TRIGGER IF NOT EXISTS capture_lifecycle_no_delete BEFORE DELETE ON capture_lifecycle_records
  BEGIN SELECT RAISE(ABORT,'capture lifecycle records are append-only'); END;

CREATE TABLE IF NOT EXISTS capture_self_tests(
  self_test_id TEXT PRIMARY KEY,
  owner_key TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('claude_code','codex')),
  source_version TEXT NOT NULL,
  source_executable_sha256 TEXT NOT NULL,
  config_sha256 TEXT NOT NULL,
  wrapper_sha256 TEXT NOT NULL,
  collector_key_id TEXT NOT NULL REFERENCES collector_keys(key_id),
  native_boundary INTEGER NOT NULL CHECK(native_boundary IN (0,1)),
  exit_zero_preserved INTEGER NOT NULL CHECK(exit_zero_preserved IN (0,1)),
  exit_seven_preserved INTEGER NOT NULL CHECK(exit_seven_preserved IN (0,1)),
  completion_pairing INTEGER NOT NULL CHECK(completion_pairing IN (0,1)),
  cleanup_complete INTEGER NOT NULL CHECK(cleanup_complete IN (0,1)),
  passed INTEGER NOT NULL CHECK(passed IN (0,1)),
  detail_json TEXT NOT NULL,
  record_sha256 TEXT NOT NULL,
  key_id TEXT NOT NULL REFERENCES collector_keys(key_id),
  signature_hex TEXT NOT NULL,
  accepted_seq INTEGER NOT NULL UNIQUE,
  accepted_at_unix_ms INTEGER NOT NULL);
CREATE TRIGGER IF NOT EXISTS capture_self_tests_no_update BEFORE UPDATE ON capture_self_tests
  BEGIN SELECT RAISE(ABORT,'capture self-tests are immutable'); END;
CREATE TRIGGER IF NOT EXISTS capture_self_tests_no_delete BEFORE DELETE ON capture_self_tests
  BEGIN SELECT RAISE(ABORT,'capture self-tests are append-only'); END;

CREATE TABLE IF NOT EXISTS capture_compatibility_profiles(
  profile_id TEXT PRIMARY KEY,
  owner_key TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('claude_code','codex')),
  source_version TEXT NOT NULL,
  source_executable_sha256 TEXT NOT NULL,
  config_sha256 TEXT NOT NULL,
  matcher_json TEXT NOT NULL,
  wrapper_sha256 TEXT NOT NULL,
  capsule_schema_version INTEGER NOT NULL,
  collector_key_id TEXT NOT NULL REFERENCES collector_keys(key_id),
  permissions_valid INTEGER NOT NULL CHECK(permissions_valid IN (0,1)),
  self_test_id TEXT NOT NULL REFERENCES capture_self_tests(self_test_id),
  measured_boundary_ref TEXT NOT NULL,
  supersedes_profile_id TEXT REFERENCES capture_compatibility_profiles(profile_id),
  profile_json TEXT NOT NULL,
  profile_sha256 TEXT NOT NULL,
  key_id TEXT NOT NULL REFERENCES collector_keys(key_id),
  signature_hex TEXT NOT NULL,
  accepted_seq INTEGER NOT NULL UNIQUE,
  accepted_at_unix_ms INTEGER NOT NULL,
  UNIQUE(owner_key,source,source_version,source_executable_sha256,config_sha256,
         wrapper_sha256,collector_key_id,profile_sha256));
CREATE TRIGGER IF NOT EXISTS capture_profiles_no_update BEFORE UPDATE ON capture_compatibility_profiles
  BEGIN SELECT RAISE(ABORT,'capture compatibility profiles are immutable'); END;
CREATE TRIGGER IF NOT EXISTS capture_profiles_no_delete BEFORE DELETE ON capture_compatibility_profiles
  BEGIN SELECT RAISE(ABORT,'capture compatibility profiles are append-only'); END;

CREATE TABLE IF NOT EXISTS capture_project_authorizations(
  authorization_id TEXT PRIMARY KEY,
  owner_key TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('claude_code','codex')),
  source_project_key TEXT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('authorized','revoked')),
  reason TEXT NOT NULL,
  record_sha256 TEXT NOT NULL,
  key_id TEXT NOT NULL REFERENCES collector_keys(key_id),
  signature_hex TEXT NOT NULL,
  accepted_seq INTEGER NOT NULL UNIQUE,
  accepted_at_unix_ms INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS idx_capture_authorization_scope
  ON capture_project_authorizations(owner_key,source,source_project_key,accepted_seq);
CREATE TRIGGER IF NOT EXISTS capture_authorizations_no_update BEFORE UPDATE ON capture_project_authorizations
  BEGIN SELECT RAISE(ABORT,'capture authorizations are immutable'); END;
CREATE TRIGGER IF NOT EXISTS capture_authorizations_no_delete BEFORE DELETE ON capture_project_authorizations
  BEGIN SELECT RAISE(ABORT,'capture authorizations are append-only'); END;

CREATE TABLE IF NOT EXISTS capture_refresh_receipts(
  refresh_id TEXT PRIMARY KEY,
  owner_key TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('claude_code','codex')),
  source_project_key TEXT NOT NULL,
  observation_capsule_id TEXT REFERENCES state_capsules(capsule_id),
  execution_count INTEGER NOT NULL,
  comparison_count INTEGER NOT NULL,
  current_count INTEGER NOT NULL,
  stale_count INTEGER NOT NULL,
  unknown_count INTEGER NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('published','rejected')),
  reason TEXT,
  accepted_seq INTEGER NOT NULL UNIQUE,
  accepted_at_unix_ms INTEGER NOT NULL);
CREATE TRIGGER IF NOT EXISTS capture_refresh_no_update BEFORE UPDATE ON capture_refresh_receipts
  BEGIN SELECT RAISE(ABORT,'capture refresh receipts are immutable'); END;
CREATE TRIGGER IF NOT EXISTS capture_refresh_no_delete BEFORE DELETE ON capture_refresh_receipts
  BEGIN SELECT RAISE(ABORT,'capture refresh receipts are append-only'); END;

-- C1.2 (A168): the lifecycle sensor's rows. One machine-scoped append-only
-- chain of verbatim hook payloads. Observations only — nothing downstream
-- reads them until C8's shadow gate (held by tests/c1_hooks_probe.rs).
CREATE TABLE IF NOT EXISTS sensor_events(
  id TEXT PRIMARY KEY,
  seq INTEGER NOT NULL UNIQUE,
  received_at TEXT NOT NULL,
  hook_event TEXT NOT NULL,
  recognized INTEGER NOT NULL,
  parse_ok INTEGER NOT NULL,
  spine_complete INTEGER NOT NULL,
  session_id TEXT,
  transcript_path TEXT,
  cwd TEXT,
  payload TEXT NOT NULL,
  payload_sha256 TEXT NOT NULL,
  prev_hash TEXT NOT NULL,
  chain_hash TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS idx_sensor_events_session ON sensor_events(session_id);

-- C6 (A169): the T4 continuation rows, indexed so the scan that took 29 s
-- over two million texts answers from ~40 index entries instead. The WHERE
-- below must stay byte-identical to session_status's query (including the
-- literal prefix) or the planner silently stops using it — pinned by an
-- EXPLAIN QUERY PLAN test in session_status.
CREATE INDEX IF NOT EXISTS idx_events_t4_continuation ON events(project_id, ts)
  WHERE kind='user_msg' AND text LIKE 'This session is being continued from a previous%';
-- C6 (A169): the per-session timestamp lookup, index-only. The RB1 tail
-- derivation asks MAX(ts) once per session; against the bare table on a
-- cold cache that measured ~30 s corpus-wide, against this index it is a
-- b-tree descent per session.
CREATE INDEX IF NOT EXISTS idx_events_session_ts ON events(session_id, ts);
CREATE TRIGGER IF NOT EXISTS sensor_events_no_update BEFORE UPDATE ON sensor_events
  BEGIN SELECT RAISE(ABORT,'sensor events are append-only'); END;
CREATE TRIGGER IF NOT EXISTS sensor_events_no_delete BEFORE DELETE ON sensor_events
  BEGIN SELECT RAISE(ABORT,'sensor events are append-only'); END;

-- C2 (A171): workspace snapshots — read-only captures of repo/tree/environment
-- state at protect boundaries. The doc is the record, content-addressed; the
-- columns exist for lookup. Append-only like every receipt.
CREATE TABLE IF NOT EXISTS workspace_snapshots(
  id TEXT PRIMARY KEY,
  taken_at TEXT NOT NULL,
  root TEXT NOT NULL,
  head TEXT,
  branch TEXT,
  complete INTEGER NOT NULL,
  doc TEXT NOT NULL,
  doc_sha256 TEXT NOT NULL);
-- C5 (A182): the operation/replay ledger. Every potentially side-effectful
-- operation the record shows, with a stable identity, an outcome, and a replay
-- class carried by a deterministic rule (RB7 proof provenance — the rule is
-- named per row in `basis`). Append-only: a re-derivation appends, and the
-- classification of a past operation is never edited under it.
CREATE TABLE IF NOT EXISTS operations(
  op_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  event_id TEXT NOT NULL,
  op_type TEXT NOT NULL,
  target TEXT,
  command_sha256 TEXT NOT NULL,
  command_preview TEXT NOT NULL,
  observed_at TEXT,
  outcome TEXT NOT NULL CHECK(outcome IN
    ('not_started','running','succeeded','failed','cancelled','unknown')),
  replay_class TEXT NOT NULL CHECK(replay_class IN
    ('safe','conditional','unsafe','unknown')),
  basis TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS idx_operations_project ON operations(project_id);
CREATE INDEX IF NOT EXISTS idx_operations_digest ON operations(command_sha256);
CREATE TRIGGER IF NOT EXISTS operations_no_update BEFORE UPDATE ON operations
  BEGIN SELECT RAISE(ABORT,'operations are append-only'); END;
CREATE TRIGGER IF NOT EXISTS operations_no_delete BEFORE DELETE ON operations
  BEGIN SELECT RAISE(ABORT,'operations are append-only'); END;

-- C4.3 (A180): the continuity handshake's results. Append-only: an override
-- is a NEW row with its reason, never an edit of the verdict it overrides.
-- This table is the denominator of the north-star metric — verified
-- continuation rate is computable from these rows and nowhere else.
CREATE TABLE IF NOT EXISTS continuity_checks(
  id TEXT PRIMARY KEY,
  checked_at TEXT NOT NULL,
  project_id TEXT NOT NULL,
  capsule_chain_position TEXT NOT NULL,
  snapshot_id TEXT,
  outcome TEXT NOT NULL CHECK(outcome IN ('verified','resumed_with_warnings','failed')),
  passed INTEGER NOT NULL,
  total INTEGER NOT NULL,
  insufficient INTEGER NOT NULL,
  detail TEXT NOT NULL,
  override_reason TEXT);
CREATE INDEX IF NOT EXISTS idx_continuity_project ON continuity_checks(project_id);
CREATE TRIGGER IF NOT EXISTS continuity_checks_no_update BEFORE UPDATE ON continuity_checks
  BEGIN SELECT RAISE(ABORT,'continuity checks are append-only'); END;
CREATE TRIGGER IF NOT EXISTS continuity_checks_no_delete BEFORE DELETE ON continuity_checks
  BEGIN SELECT RAISE(ABORT,'continuity checks are append-only'); END;

CREATE TRIGGER IF NOT EXISTS workspace_snapshots_no_update BEFORE UPDATE ON workspace_snapshots
  BEGIN SELECT RAISE(ABORT,'workspace snapshots are append-only'); END;
CREATE TRIGGER IF NOT EXISTS workspace_snapshots_no_delete BEFORE DELETE ON workspace_snapshots
  BEGIN SELECT RAISE(ABORT,'workspace snapshots are append-only'); END;

-- A199 Sessions 2-3 (DDL CHANGE, routed through `thruline upgrade` per
-- A168/A193): the speaker layer's announce-to-ledger record. One row per
-- completion claim whose own session record held no matching tool evidence
-- when `thruline audit --hook stop` looked. A row here says "claimed, and no
-- proof found in the record" - NEVER "a lie found": the claim may be entirely
-- true and merely unshown, and no reader of this table may collapse that
-- distinction (A199). Append-only on its own machine-scoped hash chain
-- (scope 'speaker'); the chain content is recomputable from the row's own
-- columns, so verification needs no stored blob. Nothing derives nodes,
-- edges, or verdicts from these rows; the digest cache and future surfaces
-- read them as history.
CREATE TABLE IF NOT EXISTS speaker_findings(
  id TEXT PRIMARY KEY,
  seq INTEGER NOT NULL UNIQUE,
  recorded_at TEXT NOT NULL,
  hook TEXT NOT NULL CHECK(hook IN ('stop','session-end')),
  session_id TEXT NOT NULL,
  project_key TEXT NOT NULL,
  transcript_path TEXT NOT NULL,
  transcript_line INTEGER NOT NULL,
  claim_kind TEXT NOT NULL CHECK(claim_kind IN ('work','marker')),
  claim_marker TEXT NOT NULL,
  claim_text TEXT NOT NULL,
  expected_evidence TEXT NOT NULL,
  evidence_rows_scanned INTEGER NOT NULL,
  sensor_seq_lo INTEGER,
  sensor_seq_hi INTEGER,
  content_sha256 TEXT NOT NULL,
  prev_hash TEXT NOT NULL,
  chain_hash TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS idx_speaker_findings_session ON speaker_findings(session_id);
CREATE INDEX IF NOT EXISTS idx_speaker_findings_project ON speaker_findings(project_key);
CREATE TRIGGER IF NOT EXISTS speaker_findings_no_update BEFORE UPDATE ON speaker_findings
  BEGIN SELECT RAISE(ABORT,'speaker findings are append-only'); END;
CREATE TRIGGER IF NOT EXISTS speaker_findings_no_delete BEFORE DELETE ON speaker_findings
  BEGIN SELECT RAISE(ABORT,'speaker findings are append-only'); END;

-- A201 (DDL CHANGE, routed through `thruline upgrade` per A168/A193): the
-- verifier's verdict record. One row per evaluation of a claim_completion
-- submission (`mcp_calls.outcome = 'ready_for_verification'`): the claim row
-- itself is append-only and is NEVER mutated — a verdict is a separate fact
-- with its own receipt, appended on its own hash chain (scope
-- 'claim_verdict'). Re-running the verifier over an unchanged record is a
-- no-op by content identity; a record that has since changed appends a NEW
-- row, and the latest row per call_id is the standing verdict. 'supported'
-- means every manifest entry matched evidence; 'no_proof_found' means the
-- searched window held no match for at least one entry — which is a fact
-- about the record, never "a lie found" (A199); 'unknown' means the record
-- could not decide (unidentifiable session, incomplete evidence window,
-- nothing checkable) and an unknown never accuses.
CREATE TABLE IF NOT EXISTS claim_verdicts(
  id TEXT PRIMARY KEY,
  seq INTEGER NOT NULL UNIQUE,
  recorded_at TEXT NOT NULL,
  call_id TEXT NOT NULL REFERENCES mcp_calls(call_id),
  verdict TEXT NOT NULL CHECK(verdict IN ('supported','no_proof_found','unknown')),
  reason TEXT NOT NULL,
  detail_json TEXT NOT NULL,
  session_id TEXT,
  evidence_rows_scanned INTEGER NOT NULL,
  sensor_seq_lo INTEGER,
  sensor_seq_hi INTEGER,
  spilled_deliveries INTEGER NOT NULL,
  content_sha256 TEXT NOT NULL,
  prev_hash TEXT NOT NULL,
  chain_hash TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS idx_claim_verdicts_call ON claim_verdicts(call_id);
CREATE TRIGGER IF NOT EXISTS claim_verdicts_no_update BEFORE UPDATE ON claim_verdicts
  BEGIN SELECT RAISE(ABORT,'claim verdicts are append-only'); END;
CREATE TRIGGER IF NOT EXISTS claim_verdicts_no_delete BEFORE DELETE ON claim_verdicts
  BEGIN SELECT RAISE(ABORT,'claim verdicts are append-only'); END;

-- A205 (MANDATE-v4 §11; DDL CHANGE routed through `thruline upgrade` per
-- A168/A193): the unified observation stream. One machine-wide append-only
-- hash chain (scope 'observation') for the mandate's observation types; the
-- closed type set, the injective canonical encoding behind the
-- content-addressed id, and the six receipt-termination arms live in
-- crates/core/src/observation.rs. The existing per-domain chains and the
-- append-only-unchained C-series tables stay authoritative for what they
-- already record; nothing is backfilled or re-homed here. source_receipt is
-- typed JSON whose Rust shape guarantees a raw-evidence terminal — a receipt
-- can point at a derived row only BESIDE one, never instead of one.
-- completeness is the closed vocabulary complete/incomplete/unknown and
-- nothing stronger: an observation states what was seen, never a verdict.
-- Writers arrive per-stage; nothing on the hook path writes here yet.
CREATE TABLE IF NOT EXISTS observations(
  id TEXT PRIMARY KEY,
  seq INTEGER NOT NULL UNIQUE,
  recorded_at TEXT NOT NULL,
  schema_version INTEGER NOT NULL,
  observation_type TEXT NOT NULL CHECK(observation_type IN (
    'WORKSPACE_OBSERVED','COMMAND_STARTED','COMMAND_FINISHED',
    'EVIDENCE_BOUND','EVIDENCE_STALE','EVIDENCE_INVALIDATED',
    'SNAPSHOT_CREATED','SNAPSHOT_VERIFIED','CAPSULE_CREATED',
    'CAPSULE_OFFERED','CAPSULE_RECEIVED','CAPSULE_DIGEST_MATCHED',
    'REPOSITORY_IDENTITY_MATCHED','WORKSPACE_IDENTITY_MATCHED',
    'OPEN_ITEM_ACKNOWLEDGED','OPEN_ITEM_REJECTED',
    'FIRST_RELATED_ACTION_OBSERVED','RECOVERY_HUMAN_ADJUDICATED',
    'INCIDENT_DETECTED','POLICY_EVALUATED','EVIDENCE_PACKET_CREATED',
    'EXTERNAL_ATTESTATION_RECEIVED')),
  source_adapter TEXT NOT NULL,
  source_session TEXT NOT NULL,
  repository_identity TEXT NOT NULL,
  workspace_identity TEXT,
  observed_at TEXT NOT NULL,
  completeness TEXT NOT NULL CHECK(completeness IN ('complete','incomplete','unknown')),
  source_receipt TEXT NOT NULL,
  prev_hash TEXT NOT NULL,
  chain_hash TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS idx_observations_session ON observations(source_session);
CREATE INDEX IF NOT EXISTS idx_observations_type ON observations(observation_type);
CREATE TRIGGER IF NOT EXISTS observations_no_update BEFORE UPDATE ON observations
  BEGIN SELECT RAISE(ABORT,'observations are append-only'); END;
CREATE TRIGGER IF NOT EXISTS observations_no_delete BEFORE DELETE ON observations
  BEGIN SELECT RAISE(ABORT,'observations are append-only'); END;

-- MANDATE-v4 §13 (A207): evidence bindings — independently captured process
-- executions bound to exact pre/post workspace identities. id is the EVB_V1
-- canonical digest (content-addressed); doc is the full JSON document with
-- its own digest, workspace_snapshots-style; the queryable columns exist so
-- "latest binding for scope X" is one indexed read, never a chain scan. Rows
-- are never edited: staleness and invalidation are observations on the
-- chained stream, which also carries each binding's digest in a hash-covered
-- EVIDENCE_BOUND receipt. Written only by foreground commands.
CREATE TABLE IF NOT EXISTS evidence_bindings(
  id TEXT PRIMARY KEY,
  seq INTEGER NOT NULL UNIQUE,
  recorded_at TEXT NOT NULL,
  schema_version INTEGER NOT NULL,
  scope_class TEXT NOT NULL CHECK(scope_class IN ('test','build','unclassified')),
  scope_key TEXT NOT NULL,
  root TEXT NOT NULL,
  repository_identity TEXT NOT NULL,
  pre_identity TEXT,
  post_identity TEXT,
  exit_code INTEGER,
  term_signal INTEGER,
  bound_at TEXT NOT NULL,
  source_session TEXT NOT NULL,
  doc TEXT NOT NULL,
  doc_sha256 TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS idx_evidence_bindings_scope
  ON evidence_bindings(scope_key, seq);
CREATE INDEX IF NOT EXISTS idx_evidence_bindings_repo
  ON evidence_bindings(repository_identity, seq);
CREATE TRIGGER IF NOT EXISTS evidence_bindings_no_update BEFORE UPDATE ON evidence_bindings
  BEGIN SELECT RAISE(ABORT,'evidence bindings are append-only'); END;
CREATE TRIGGER IF NOT EXISTS evidence_bindings_no_delete BEFORE DELETE ON evidence_bindings
  BEGIN SELECT RAISE(ABORT,'evidence bindings are append-only'); END;

-- T-2-1 (DDL CHANGE, routed through `thruline upgrade` per A168/A193; brief
-- B3.3-B3.7, B3.12, B6.1, B11.4): the Change layer.
--
-- A Change is a bounded unit of intent inside a project. It is a CHILD OF THE
-- PROJECT and a SIBLING OF THE SESSION, never a child of one: B3.10 and B3.11
-- (a Change may span sessions; a session may contribute to several Changes)
-- make any session-keyed Change wrong by construction.
--
-- Three rules govern every table below, and the compatibility tests pin each.
--
-- 1. **It stores no evidence.** Every reference to a session, an event or a
--    snapshot is the existing id and nothing else - no text, no timestamp and
--    no digest is copied out of `events`. Asking what evidence a Change has is
--    a join, so there is no second store to disagree with the first (B2.5).
-- 2. **Nothing derived points back.** No verdict, claim, check or receipt row
--    carries a change id. A Change therefore CANNOT inherit a session's result:
--    the inheritance path does not exist to be closed (B3.13).
-- 3. **Append-only, like everything beside it.** Re-scoping, re-binding and
--    confirming all APPEND; nothing is edited in place. The standing value is
--    the highest version (scope, binding) or the latest row (association).
--
-- Cost (L1.1, Rule 13), MEASURED on a fixture store rather than estimated -
-- `crates/core/tests/change_cost.rs` re-measures it on every run and fails if
-- it has left the band. N = 1000, PRAGMA page_count x page_size, checkpointed
-- and vacuumed between readings, on 2026-09-17:
--
--   one Change  = 2,216 B  (request prose at the store's measured mean of
--                           914 B, five criteria, one acceptance record)
--   one session-to-Change association = 410 B
--
-- For scale: the live personal ledger is 7.79 GB over 76,205 sessions and 96
-- projects. One association for every existing session would add ~31 MB, and
-- fifty Changes in every project ~11 MB - together about 0.5 % of what is
-- already there. This is the cost of the Change LAYER only. Check runs,
-- artifacts, review rounds and packet versions (B3.8, B3.9) are not here and
-- will dominate; this machine has no measurement of those at all.

CREATE TABLE IF NOT EXISTS changes(
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id),
  opened_at TEXT NOT NULL,
  -- The request (B3.4). `request_event_id` is the original prompt or spec BY
  -- ID: the prose stays in `events`, under the append-only triggers, and a
  -- receipt path from a Change to raw JSONL is the same path a session has.
  title TEXT NOT NULL CHECK(length(trim(title)) > 0),
  purpose TEXT NOT NULL,
  request_event_id TEXT REFERENCES events(id),
  requester TEXT NOT NULL CHECK(length(trim(requester)) > 0),
  request_source TEXT NOT NULL CHECK(request_source IN ('person','agent','imported')));
CREATE INDEX IF NOT EXISTS idx_changes_project ON changes(project_id, opened_at, id);
CREATE TRIGGER IF NOT EXISTS changes_request_event_in_project
  BEFORE INSERT ON changes
  WHEN NEW.request_event_id IS NOT NULL AND NOT EXISTS(
         SELECT 1 FROM events e
          WHERE e.id=NEW.request_event_id AND e.project_id=NEW.project_id)
  BEGIN SELECT RAISE(ABORT,'a change request must point at an event in its own project'); END;
CREATE TRIGGER IF NOT EXISTS changes_no_replace BEFORE INSERT ON changes
  WHEN EXISTS(SELECT 1 FROM changes old WHERE old.id=NEW.id)
  BEGIN SELECT RAISE(ABORT,'changes are append-only'); END;
CREATE TRIGGER IF NOT EXISTS changes_no_update BEFORE UPDATE ON changes
  BEGIN SELECT RAISE(ABORT,'changes are append-only'); END;
CREATE TRIGGER IF NOT EXISTS changes_no_delete BEFORE DELETE ON changes
  BEGIN SELECT RAISE(ABORT,'changes are append-only'); END;

-- The accepted scope, VERSIONED (B3.5). Re-scoping appends version n+1 and
-- leaves n readable, because B6.13's "any approval remains attached to its
-- original target version" needs the old version to still be there to be
-- attached to. `content_sha256` is over the canonical JSON of this row's five
-- scope fields, so "is this the scope that was accepted" is an equality test
-- rather than a diff.
CREATE TABLE IF NOT EXISTS change_scope_revisions(
  change_id TEXT NOT NULL REFERENCES changes(id),
  scope_version INTEGER NOT NULL CHECK(scope_version >= 1),
  proposed_at TEXT NOT NULL,
  criteria_json TEXT NOT NULL,
  exclusions_json TEXT NOT NULL,
  allowed_scope_json TEXT NOT NULL,
  constraints_json TEXT NOT NULL,
  check_profile_json TEXT NOT NULL,
  content_sha256 TEXT NOT NULL,
  PRIMARY KEY(change_id, scope_version));
CREATE TRIGGER IF NOT EXISTS change_scope_revisions_no_replace
  BEFORE INSERT ON change_scope_revisions
  WHEN EXISTS(SELECT 1 FROM change_scope_revisions old
               WHERE old.change_id=NEW.change_id AND old.scope_version=NEW.scope_version)
  BEGIN SELECT RAISE(ABORT,'accepted scope revisions are append-only'); END;
CREATE TRIGGER IF NOT EXISTS change_scope_revisions_no_update
  BEFORE UPDATE ON change_scope_revisions
  BEGIN SELECT RAISE(ABORT,'accepted scope revisions are append-only'); END;
CREATE TRIGGER IF NOT EXISTS change_scope_revisions_no_delete
  BEFORE DELETE ON change_scope_revisions
  BEGIN SELECT RAISE(ABORT,'accepted scope revisions are append-only'); END;

-- The acceptance record (B3.5), and it is NOT an approval. Accepting a scope
-- says "this is what counts as done"; approving says "the evidence supports
-- this version". Keeping them in different tables is what stops a screen from
-- reading one as the other. One acceptance per scope version, one-shot: a
-- second thought is a new scope version with its own acceptance.
CREATE TABLE IF NOT EXISTS change_scope_acceptances(
  change_id TEXT NOT NULL,
  scope_version INTEGER NOT NULL,
  accepted_by TEXT NOT NULL CHECK(length(trim(accepted_by)) > 0),
  accepted_at TEXT NOT NULL,
  rationale TEXT NOT NULL,
  PRIMARY KEY(change_id, scope_version),
  FOREIGN KEY(change_id, scope_version)
    REFERENCES change_scope_revisions(change_id, scope_version));
CREATE TRIGGER IF NOT EXISTS change_scope_acceptances_no_replace
  BEFORE INSERT ON change_scope_acceptances
  WHEN EXISTS(SELECT 1 FROM change_scope_acceptances old
               WHERE old.change_id=NEW.change_id AND old.scope_version=NEW.scope_version)
  BEGIN SELECT RAISE(ABORT,'a scope acceptance is one-shot and append-only'); END;
CREATE TRIGGER IF NOT EXISTS change_scope_acceptances_no_update
  BEFORE UPDATE ON change_scope_acceptances
  BEGIN SELECT RAISE(ABORT,'a scope acceptance is one-shot and append-only'); END;
CREATE TRIGGER IF NOT EXISTS change_scope_acceptances_no_delete
  BEFORE DELETE ON change_scope_acceptances
  BEGIN SELECT RAISE(ABORT,'a scope acceptance is one-shot and append-only'); END;

-- The implementation binding (B3.7, B6.1). ONE snapshot per binding version,
-- deliberately: the failure mode this task exists to prevent is a Change that
-- points at two snapshots at once and therefore says "approved" for whichever
-- one a query happened to sort first. A new snapshot is a new binding version;
-- the older versions stay readable as the series B3.7 asks for, and the
-- standing one is the highest `binding_version`. Repository and workspace
-- identity are the EXISTING identity model's strings (crates/core/identity.rs,
-- workspace_identities), not a second identity.
CREATE TABLE IF NOT EXISTS change_implementations(
  change_id TEXT NOT NULL REFERENCES changes(id),
  binding_version INTEGER NOT NULL CHECK(binding_version >= 1),
  bound_at TEXT NOT NULL,
  repository_identity TEXT NOT NULL CHECK(length(trim(repository_identity)) > 0),
  base_revision TEXT NOT NULL CHECK(length(trim(base_revision)) > 0),
  snapshot_id TEXT REFERENCES workspace_snapshots(id),
  workspace_identity TEXT,
  PRIMARY KEY(change_id, binding_version));
CREATE TRIGGER IF NOT EXISTS change_implementations_no_replace
  BEFORE INSERT ON change_implementations
  WHEN EXISTS(SELECT 1 FROM change_implementations old
               WHERE old.change_id=NEW.change_id AND old.binding_version=NEW.binding_version)
  BEGIN SELECT RAISE(ABORT,'implementation bindings are append-only'); END;
CREATE TRIGGER IF NOT EXISTS change_implementations_no_update
  BEFORE UPDATE ON change_implementations
  BEGIN SELECT RAISE(ABORT,'implementation bindings are append-only'); END;
CREATE TRIGGER IF NOT EXISTS change_implementations_no_delete
  BEFORE DELETE ON change_implementations
  BEGIN SELECT RAISE(ABORT,'implementation bindings are append-only'); END;

-- session <-> Change association with explicit provenance (B3.12). One
-- append-only row per assertion; the standing provenance for a pair is its
-- latest row, so confirming a suggestion APPENDS a confirmation and never
-- edits the suggestion into one. `basis` says why the tool thinks so.
--
-- The CHECK is the refusal B3.12 asks for, in the database rather than in a
-- caller: a row may only be 'confirmed' when its basis is 'user' AND it names
-- a non-empty actor. Timing, filename overlap and an agent's own declaration
-- can propose and can never confirm, so no suggestion becomes a confirmation
-- without an explicit human act.
--
-- Nothing here carries a result. An association is two ids, a word and a time;
-- no check outcome, verdict or claim state is copied onto a Change by it
-- (B3.13). T-2-2 builds the suggestion logic; this is the table and the
-- refusal.
--
-- T-2-2 (DDL CHANGE, routed through `thruline upgrade` per A168/A193) amends
-- this one table in two ways, and NOTHING ELSE IN THE SCHEMA:
--
--   a. `provenance` gains 'rejected'. T-2-1 built confirm; B3.12 and the T-2-2
--      card need the other half of the explicit act — "no, this session does
--      not belong here" — and a rejection that could only be expressed by the
--      ABSENCE of a confirmation is a rejection nobody can see, re-suggest
--      against, or attribute to a person. A rejection is held to exactly the
--      same bar as a confirmation: basis 'user' and a named actor.
--   b. `reason` is added. The card requires the act to carry the person's own
--      words; a reason column on an append-only row is the only place they can
--      live without editing the suggestion they answer.
--
-- This table has NEVER SHIPPED: HEAD (5cab107d) has no `change_session_...`
-- table at all, T-2-1 is uncommitted, and so no database on disk carries the
-- earlier two-word shape. A store that somehow does — a developer tree built
-- from the intermediate working copy — would keep its old CHECK, because
-- `CREATE TABLE IF NOT EXISTS` is a no-op against an existing table, and would
-- then refuse every rejection with a constraint error nobody could read. That
-- is the silent-wrong-schema failure, so `assert_association_shape` below
-- refuses to open such a store and says which table and which word is missing.
CREATE TABLE IF NOT EXISTS change_session_associations(
  id TEXT PRIMARY KEY,
  change_id TEXT NOT NULL REFERENCES changes(id),
  session_id TEXT NOT NULL REFERENCES sessions(id),
  provenance TEXT NOT NULL CHECK(provenance IN ('suggested','confirmed','rejected')),
  basis TEXT NOT NULL CHECK(basis IN ('user','timing','filename','agent-declared')),
  actor TEXT,
  asserted_at TEXT NOT NULL,
  reason TEXT,
  CHECK(provenance='suggested'
        OR (provenance IN ('confirmed','rejected') AND basis='user'
            AND actor IS NOT NULL AND length(trim(actor)) > 0)));
CREATE INDEX IF NOT EXISTS idx_change_session_associations_change
  ON change_session_associations(change_id, asserted_at, id);
CREATE INDEX IF NOT EXISTS idx_change_session_associations_session
  ON change_session_associations(session_id, asserted_at, id);
CREATE TRIGGER IF NOT EXISTS change_session_associations_same_project
  BEFORE INSERT ON change_session_associations
  WHEN NOT EXISTS(
         SELECT 1 FROM changes c JOIN sessions s ON s.project_id=c.project_id
          WHERE c.id=NEW.change_id AND s.id=NEW.session_id)
  BEGIN SELECT RAISE(ABORT,'a change and its session must share the exact project'); END;
CREATE TRIGGER IF NOT EXISTS change_session_associations_no_replace
  BEFORE INSERT ON change_session_associations
  WHEN EXISTS(SELECT 1 FROM change_session_associations old WHERE old.id=NEW.id)
  BEGIN SELECT RAISE(ABORT,'change associations are append-only'); END;
CREATE TRIGGER IF NOT EXISTS change_session_associations_no_update
  BEFORE UPDATE ON change_session_associations
  BEGIN SELECT RAISE(ABORT,'change associations are append-only'); END;
CREATE TRIGGER IF NOT EXISTS change_session_associations_no_delete
  BEFORE DELETE ON change_session_associations
  BEGIN SELECT RAISE(ABORT,'change associations are append-only'); END;

-- T-3-1 (DDL CHANGE, routed through `thruline upgrade` per A168/A193; brief
-- B4.6, B5.6, B7.5-B7.7, B10.2, Q3): criterion <-> evidence.
--
-- ONE table, and it holds LINKS, never evidence. Every row is
-- (which criterion, which already-stored evidence row, why the link exists).
-- No outcome, no count, no prose copied out of the thing it points at: asking
-- "what did this show" is a join into `evidence_bindings` / `speaker_findings`
-- / `observations`, so there is no second store to disagree with the first
-- (B2.5, the same rule the Change tables above are built on).
--
-- `basis` is a CLOSED four-word set and the absence of a fifth is the point:
--
--   'declared'    the accepted check profile names this check for this
--                 criterion's class.
--   'named'       the evidence's own typed name field carries a subject token
--                 of the criterion.
--   'manual'      a named person said so, with the time they said it.
--   'not-bearing' a named person said a link does NOT bear. A TOMBSTONE: the
--                 row it answers is still there, still readable, still
--                 attributable. Unlinking never deletes.
--
-- There is no 'timing' basis, in this CHECK or anywhere in the code that
-- writes it. A run that happened during the same session as the work is not
-- evidence about a criterion, and the failure this whole table exists to stop
-- is exactly that: `cargo test` shown under a criterion it never touched
-- because the clock agreed. The constraint is in the database so a caller that
-- bypasses the module is refused identically.
--
-- 'manual' and 'not-bearing' both REQUIRE an actor: an act nobody signed is
-- not an act (B5.6). 'declared' and 'named' forbid one, because a rule did it
-- and attributing a rule's output to a person is a lie about who decided.
--
-- Append-only, like everything beside it. A link that turned out to be wrong
-- is answered by a later row, never edited away; the standing state of a pair
-- is its latest row by (asserted_at, id).
CREATE TABLE IF NOT EXISTS change_criterion_evidence(
  id TEXT PRIMARY KEY,
  change_id TEXT NOT NULL REFERENCES changes(id),
  scope_version INTEGER NOT NULL,
  criterion_index INTEGER NOT NULL CHECK(criterion_index >= 1),
  evidence_kind TEXT NOT NULL CHECK(evidence_kind IN ('check_run','finding','observation')),
  evidence_id TEXT NOT NULL CHECK(length(trim(evidence_id)) > 0),
  session_id TEXT NOT NULL REFERENCES sessions(id),
  basis TEXT NOT NULL CHECK(basis IN ('declared','named','manual','not-bearing')),
  -- What matched, as the rule saw it: the profile entry that declared it, or
  -- the subject token that was found in the evidence's own name. Never a
  -- sentence, never a summary of the evidence.
  detail TEXT,
  actor TEXT,
  asserted_at TEXT NOT NULL,
  reason TEXT,
  CHECK(basis IN ('declared','named')
        OR (actor IS NOT NULL AND length(trim(actor)) > 0)),
  CHECK(basis IN ('manual','not-bearing') OR actor IS NULL),
  FOREIGN KEY(change_id, scope_version)
    REFERENCES change_scope_revisions(change_id, scope_version));
CREATE INDEX IF NOT EXISTS idx_change_criterion_evidence_criterion
  ON change_criterion_evidence(change_id, scope_version, criterion_index, asserted_at, id);
CREATE INDEX IF NOT EXISTS idx_change_criterion_evidence_evidence
  ON change_criterion_evidence(evidence_kind, evidence_id);
CREATE TRIGGER IF NOT EXISTS change_criterion_evidence_same_change
  BEFORE INSERT ON change_criterion_evidence
  WHEN NOT EXISTS(
         SELECT 1 FROM changes c JOIN sessions s ON s.project_id=c.project_id
          WHERE c.id=NEW.change_id AND s.id=NEW.session_id)
  BEGIN SELECT RAISE(ABORT,'a criterion link and its session must share the exact project'); END;
CREATE TRIGGER IF NOT EXISTS change_criterion_evidence_no_replace
  BEFORE INSERT ON change_criterion_evidence
  WHEN EXISTS(SELECT 1 FROM change_criterion_evidence old WHERE old.id=NEW.id)
  BEGIN SELECT RAISE(ABORT,'criterion links are append-only'); END;
CREATE TRIGGER IF NOT EXISTS change_criterion_evidence_no_update
  BEFORE UPDATE ON change_criterion_evidence
  BEGIN SELECT RAISE(ABORT,'criterion links are append-only'); END;
CREATE TRIGGER IF NOT EXISTS change_criterion_evidence_no_delete
  BEFORE DELETE ON change_criterion_evidence
  BEGIN SELECT RAISE(ABORT,'criterion links are append-only'); END;

-- T-3-2 (DDL CHANGE, routed through `thruline upgrade` per A168/A193; brief
-- B6.1-B6.3, B4.6, Q4): what exactly was examined.
--
-- ONE table, and it holds a REFERENCE to what was examined, never a copy of
-- it. `snapshot_id` is `workspace_snapshots.id`; `source_identity` is
-- `workspace_identity`'s `wsid1:...`, which is the tree hash INCLUDING
-- uncommitted changes; `lockfiles_sha256` is over the digests the snapshot
-- already computed. B6.1 says extend the identity and snapshot model rather
-- than design a competing one, so no field here is a second way of saying
-- something the store already says.
--
-- `subject_kind` says which examination this record belongs to:
--
--   'intended'    what a Change says WILL be examined - the Prepare view's own
--                 row, and the thing an observation is compared against.
--   'check_run'   one captured command execution (`evidence_bindings.id`).
--   'observation' one recorded observation (`observations.id`).
--
-- UNKNOWN IS A STATE, NEVER A VALUE. A probe that did not answer stores SQL
-- NULL. The CHECKs at the bottom refuse the literal string `unknown` in every
-- column that could carry one, so a caller that writes the word instead of the
-- state is refused by the database rather than by a convention - and the
-- refusal is the mechanical form of "missing evidence never renders as pass".
--
-- Append-only, like everything beside it. A machine that is examined again
-- appends a second record; the standing one is the latest by rowid, and the
-- earlier one stays readable because B6.7 needs the old result to still be
-- there to be evaluated.
CREATE TABLE IF NOT EXISTS examination_inputs(
  id TEXT PRIMARY KEY,
  subject_kind TEXT NOT NULL CHECK(subject_kind IN ('intended','check_run','observation')),
  subject_id TEXT NOT NULL CHECK(length(trim(subject_id)) > 0),
  recorded_at TEXT NOT NULL,
  schema_version INTEGER NOT NULL,
  snapshot_id TEXT REFERENCES workspace_snapshots(id),
  source_identity TEXT,
  base_revision TEXT,
  uncommitted_state TEXT NOT NULL CHECK(uncommitted_state IN ('clean','present','unknown')),
  uncommitted_files INTEGER,
  uncommitted_sha256 TEXT,
  worktree_path TEXT,
  lockfiles_sha256 TEXT,
  config_sha256 TEXT,
  config_files_json TEXT NOT NULL,
  tools_json TEXT NOT NULL,
  target_kind TEXT NOT NULL CHECK(target_kind IN ('none','running_app')),
  app_port INTEGER,
  app_owner_pid INTEGER,
  app_owner_exe TEXT,
  app_database TEXT,
  -- An examination that looked at no running application carries no facts
  -- about one, and one that did must at least name the port it looked at.
  CHECK(target_kind='running_app'
        OR (app_port IS NULL AND app_owner_pid IS NULL
            AND app_owner_exe IS NULL AND app_database IS NULL)),
  CHECK(target_kind='none' OR app_port IS NOT NULL),
  -- A clean tree and an unreadable one are different facts.
  CHECK(uncommitted_state='present'
        OR (uncommitted_files IS NULL AND uncommitted_sha256 IS NULL)),
  -- `unknown` is a state, never a value.
  CHECK(snapshot_id IS NULL OR snapshot_id <> 'unknown'),
  CHECK(source_identity IS NULL OR source_identity <> 'unknown'),
  CHECK(base_revision IS NULL OR base_revision <> 'unknown'),
  CHECK(worktree_path IS NULL OR worktree_path <> 'unknown'),
  CHECK(lockfiles_sha256 IS NULL OR lockfiles_sha256 <> 'unknown'),
  CHECK(config_sha256 IS NULL OR config_sha256 <> 'unknown'),
  CHECK(app_owner_exe IS NULL OR app_owner_exe <> 'unknown'),
  CHECK(app_database IS NULL OR app_database <> 'unknown'));
CREATE INDEX IF NOT EXISTS idx_examination_inputs_subject
  ON examination_inputs(subject_kind, subject_id, recorded_at);
-- A record for an examination that does not exist would be a claim about
-- nothing. The Change's own intended record points at the Change.
CREATE TRIGGER IF NOT EXISTS examination_inputs_subject_exists
  BEFORE INSERT ON examination_inputs
  WHEN (NEW.subject_kind='check_run'
        AND NOT EXISTS(SELECT 1 FROM evidence_bindings b WHERE b.id=NEW.subject_id))
    OR (NEW.subject_kind='observation'
        AND NOT EXISTS(SELECT 1 FROM observations o WHERE o.id=NEW.subject_id))
    OR (NEW.subject_kind='intended'
        AND NOT EXISTS(SELECT 1 FROM changes c WHERE c.id=NEW.subject_id))
  BEGIN SELECT RAISE(ABORT,'an examined-inputs record must name an examination that exists'); END;
CREATE TRIGGER IF NOT EXISTS examination_inputs_no_replace
  BEFORE INSERT ON examination_inputs
  WHEN EXISTS(SELECT 1 FROM examination_inputs old WHERE old.id=NEW.id)
  BEGIN SELECT RAISE(ABORT,'examined-inputs records are append-only'); END;

CREATE TRIGGER IF NOT EXISTS examination_inputs_no_update
  BEFORE UPDATE ON examination_inputs
  BEGIN SELECT RAISE(ABORT,'examined-inputs records are append-only'); END;
CREATE TRIGGER IF NOT EXISTS examination_inputs_no_delete
  BEFORE DELETE ON examination_inputs
  BEGIN SELECT RAISE(ABORT,'examined-inputs records are append-only'); END;
