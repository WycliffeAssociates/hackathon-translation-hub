// SPIKE: types-only cut of Sefer #core/observability (HEAD 50aa9cd), lines 1-273 minus the Effect import.
// The Effect service (`Observability`, `ObservabilityLive`) is NOT copied: the editor imports only types.

/**
 * How a piece of work or a decision came out. An outcome, not a severity.
 *
 * Two of them mean "it did not happen", and the difference is whose fault:
 *
 *   - `failed` — OUR code or an invariant broke. The one alarm: the dev
 *     console prints it unasked, and nothing else it prints unasked.
 *   - `unavailable` — the world said no: offline, a server down or slow, a
 *     timeout, a host without the capability. Expected in the field, so it is
 *     kept and exported but does not shout. Set only at the outside
 *     boundaries (the Remote port, the catalogue, the language API, the
 *     updater, `fetch`); anything uncaught anywhere else stays `failed`, so a
 *     bug cannot hide here by default.
 *
 * `refused` and `declined` are usually the app working — a rule holding, a
 * person choosing no — and are not warnings.
 */
export type Verdict =
  | "ready"
  | "passed"
  | "refused"
  | "rewrote"
  | "consumed"
  | "declined"
  | "unavailable"
  | "failed";

/**
 * The verdicts the failure ring keeps, so a burst of ordinary work cannot
 * overwrite the one event someone will ask about.
 */
const HELD_VERDICTS: ReadonlySet<Verdict> = new Set<Verdict>(["failed", "unavailable", "refused"]);

export type EventKind = "operation" | "span" | "note" | "log";

export type Level = "off" | "verdicts" | "spans" | "all";

/**
 * The fields of a wide event.
 *
 * Primitives ONLY, and the type is the enforcement. An attribute holding a
 * reference — an `EditorState`, a `Book`, a decoded archive — would be pinned
 * alive by the ring for the next `capacity` events, which is how a bounded
 * buffer turns into a leak. Numbers also cost less than the sentences they
 * replace: a note that built `${id} r${before} -> r${after}` on every accepted
 * edit allocated a string per keystroke to say what four fields say.
 */
export type Attrs = Readonly<Record<AttrName, string | number | boolean>>;

/**
 * A field name on a wide event.
 *
 * A plain alias, not a branded type and not a constants module. The value of a
 * wide event is that adding a field costs nothing — the moment a new field
 * needs an enum entry, people stop adding them and go back to building
 * sentences, which is the habit this whole model removes. What the alias buys
 * is what a brand would have: every filterable field in the codebase is one
 * search for `AttrName`, with no casts at the call sites to pay for it.
 *
 * Convention, not enforcement: dotted namespaces, and the SHARED ones spelled
 * the same everywhere — `book.id`, `project.root`, `fs.path`, `fs.bytes`,
 * `op.trigger`, `op.link`. A `bookId` in one file and a `book.id` in another
 * makes an attribute filter quietly miss half its data.
 */
export type AttrName = string;

/**
 * Every operation the application can open.
 *
 * A union, unlike the fields: there are a dozen, they are structural, and a
 * typo in one is invisible — it does not fail, it just never matches a filter
 * again. Adding a name here is the deliberate act of saying a new kind of work
 * exists. See `documentation/architecture/observability.md`, "Operations".
 */
export type OperationName =
  | "boot"
  | "project.open"
  | "project.close"
  | "editor.mutation"
  | "editor.selection"
  | "editor.render"
  | "save"
  | "analysis.pass"
  | "analysis.warm"
  /** One debounced flush of the recovery journals; `op.cause` is the edit that armed it. */
  | "journal.write"
  | "journal.restore"
  | "journal.discard"
  | "journal.offer"
  | "import.resource"
  | "import.remote"
  /** The Find Project table's one read of the catalogue. */
  | "catalogue.browse"
  | "find.run"
  /** One book's hits again, after an edit in one of its cards. */
  | "find.retake"
  /** Jumping from the Findings panel to a finding's span in the editor. */
  | "findings.navigate"
  /** One bound reference read and measured against the open book. */
  | "reference.load"
  /** One comparison on /review: the book diff and the engine's decision units. */
  | "review.compare"
  | `review.diff.${string}`
  /** One reading of the repository against the cloud; ends with the state. */
  | "sync.survey"
  /** What a pull would change, worked out after a survey found the device behind. */
  | "sync.plan"
  | "sync.transfer"
  /** Asking the shared project what changed: on open, or after a refused send. */
  | "sync.check"
  /** Asking a project's Writer upstreams for new work, after the shared project's check. */
  | "upstream.check"
  | "migrate.open"
  /** Writer books becoming one project: their replayed history, then the assembly commit. */
  | "migrate.assemble"
  /** Bringing a newest-version clone's older history to the device, from History. */
  | "history.deepen"
  /** The key-terms guide for one locale, decoded and listed. */
  | "terms.load"
  /** One term's references mapped onto the project, and its source side read. */
  | "terms.select"
  /** The character inventory screen, from mount to its first paint. */
  | "inventory.load"
  | "update.check"
  | "update.install"
  /** Settings → Advanced → Export diagnostics: its size, never its content. */
  | "diagnostics.export"
  /**
   * One command run from the palette, a keybinding or a click. The id is in
   * the name so a trace list reads as what the person did, and `command.` is a
   * prefix a filter can take whole.
   */
  | `command.${string}`
  /**
   * A dev playground experiment's own gestures (`src/dev/playground`): the
   * prototype is measured by the same ring the screen it may become would be.
   */
  | `playground.${string}`
  /**
   * The history prototype (`src/dev/playground/history`): its index, its
   * walks and its comparisons, measured by the ring the screen would use.
   */
  | `history.${string}`;

/**
 * One record in the ring.
 *
 * `trace` is the OPERATION — one end-to-end piece of work, which is to say one
 * thing the user did. It is not a book id and not a file: those are attributes
 * OF the work. `id` and `parent` are this record's own span identity, so the
 * events of one operation reassemble into a tree rather than a pile.
 */
export interface ObservabilityEvent {
  readonly seq: number;
  readonly t: number;
  readonly kind: EventKind;
  readonly name: string;
  readonly verdict?: Verdict;
  readonly detail?: string;
  readonly ms?: number;
  readonly self?: number;
  /** 32 hex: the operation every event of one gesture shares. */
  readonly trace?: string;
  /** 16 hex: this record's own span, when it has one. */
  readonly id?: string;
  /** 16 hex: the span this one happened inside. */
  readonly parent?: string;
  readonly attrs?: Attrs;
}

/**
 * Host code that sees every event as it is recorded. Synchronous and on the
 * hot path: a sink that needs the JSONL line calls `eventLine` itself, ideally
 * later and in a batch — serialising every event for sinks that never read the
 * text was a `JSON.stringify` per keystroke span.
 */
export type ObservabilitySink = (event: ObservabilityEvent) => void;

/**
 * What every event of a session has in common, stamped ONCE.
 *
 * The static half of the context — build, host, session — belongs on the OTLP
 * Resource and on one JSONL header record, not repeated on every line. What
 * varies goes in `attrs`; what cannot vary lives here.
 */
export interface SessionInfo {
  readonly id: string;
  readonly build?: string;
  readonly host?: string;
  readonly started: number;
}

/**
 * The narrating end of one operation.
 *
 * It IS an `ObservabilityService`, which is the whole trick: core asks the
 * context for `Observability` and cannot tell whether it received the root or
 * a gesture's own. So the gesture door provides this one for the duration of
 * the work — `Effect.provideService(Observability, op)` — and every `note()`
 * and `span()` underneath lands in the right operation across every await,
 * with no core signature growing a parameter and no core module learning that
 * tracing exists.
 */
export interface Operation extends ObservabilityService {
  readonly trace: string;
  readonly id: string;
  /** Add fields to the wide event, any time before it ends. */
  readonly attr: (attrs: Attrs) => void;
  /**
   * What the work turned out to be, when that is only known once it is done —
   * an editor transaction is a mutation or a selection move depending on what
   * the rules did to it, and the wide record is written at the end anyway.
   */
  readonly rename: (name: OperationName) => void;
  /**
   * Writes the one wide record. Returns its duration in milliseconds.
   *
   * `ms` overrides the measured wall time for a caller that knows better: an
   * instrument whose trace is closed lazily would otherwise report the idle
   * time until the next gesture as the duration of this one.
   */
  readonly end: (verdict?: Verdict, attrs?: Attrs, ms?: number) => number;
}

export interface ObservabilityService {
  /**
   * Open an operation: one user-initiated piece of work, or one piece of
   * background work that no gesture caused.
   *
   * `cause` is the trace of the work this one FOLLOWED FROM, when that work has
   * already finished — a debounced journal write, an analysis pass serving
   * several keystrokes. It is not a parent: a parent would have to pick one of
   * those keystrokes and be wrong about the rest, and the gesture's own record
   * could never be written while it waited. `op.cause` carries it as a field,
   * so one query returns the whole cascade.
   */
  readonly operation: (
    name: OperationName,
    attrs?: Attrs,
    options?: { readonly cause?: string | undefined },
  ) => Operation;
  readonly span: (name: string, note?: string, attrs?: Attrs) => (attrs?: Attrs) => number;
  readonly note: (rule: string, verdict: Verdict, detail?: string, attrs?: Attrs) => void;
  readonly recent: (limit?: number) => readonly ObservabilityEvent[];
  /**
   * The newest events whose verdict is in `HELD_VERDICTS`, oldest first, from
   * a ring of their own (`FAILURE_CAPACITY`). Kept at recording time, so the
   * main ring wrapping never loses one.
   */
  readonly failures: (limit?: number) => readonly ObservabilityEvent[];
  readonly export: () => string;
  readonly level: () => Level;
  readonly setLevel: (level: Level) => void;
  readonly dropped: () => number;
  readonly session: () => SessionInfo;
}

export interface ObservabilityOptions {
  readonly capacity?: number;
  readonly level?: Level;
  readonly sink?: ObservabilitySink | undefined;
  readonly build?: string | undefined;
  readonly host?: string | undefined;
}

const DEFAULT_CAPACITY = 2000;

const FAILURE_CAPACITY = 200;

const DEFAULT_LEVEL: Level = "all";

