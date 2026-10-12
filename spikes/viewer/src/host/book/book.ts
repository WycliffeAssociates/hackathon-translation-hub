/**
 * SPIKE stub of Sefer #core/book/book (HEAD 50aa9cd).
 *
 * Copied verbatim: the vocabulary types (BookId, Origin, Trust, Receipt,
 * History, ChangeListener, Book) and the pure helpers UNTRUSTED,
 * asChangeList, makeListeners.
 *
 * STUBBED: `Refusal` is an `effect` `Data.TaggedError` in Sefer. Here it is a
 * plain Error subclass with the same tag and fields, so the editor's
 * `instanceof Refusal` (satellite.ts) still works without pulling Effect.
 * NOT copied: makeBook/openBook/applyAll (Effect FileSystem, Source.apply) —
 * write paths a read-only viewer never reaches.
 */
import type { Result } from "effect";

import type { Change, Source, SourceStamp } from "../source/source";

/** The `\id` code when the text declares one, otherwise the file stem. */
export type BookId = string;

/**
 * Who asked for an edit. Trusted origins (fix, format, project.*, recovery,
 * revert) bypass the keyboard guards the way a fix-it does; untrusted ones are
 * judged by every rule. The editor records the origin as CodeMirror's
 * `userEvent`, so it is a string rather than an enum.
 */
export type Origin =
  | "keyboard"
  | "paste"
  | "window"
  | "fix"
  | "format"
  | "overlay"
  | "replace"
  | "recovery"
  | "revert"
  /** Text a receive brought in: the file moved forward, and the Book with it. */
  | "incoming"
  | `project.${string}`
  | (string & {});

export type Trust = { readonly trusted: false } | { readonly trusted: true; readonly by: string };

export const UNTRUSTED: Trust = { trusted: false };

export const trustedBy = (by: string): Trust => ({ trusted: true, by });

/** What an accepted edit did to a Book, in stamps. Refusals carry no receipt. */
export interface Receipt {
  readonly before: SourceStamp;
  readonly after: SourceStamp;
  readonly origin: Origin;
}

/**
 * Why an edit did not land. `rule` names the admission rule (or `source.apply`
 * for the plain Book's range checks) so a trace can say which door closed;
 * `reason` is that rule's own vocabulary.
 */
export class Refusal extends Error {
  readonly _tag = "Refusal" as const;
  readonly rule: string;
  readonly reason: string;
  readonly description: string;
  constructor(args: { readonly rule: string; readonly reason: string; readonly description: string }) {
    super(args.description);
    this.rule = args.rule;
    this.reason = args.reason;
    this.description = args.description;
  }
}
/** Undo history, present only when a CodeMirror state holds the text. */
export interface History {
  undo(): boolean;
  redo(): boolean;
  depth(): { readonly undo: number; readonly redo: number };
}

export type ChangeListener = (receipt: Receipt, changes: readonly Change[]) => void;

export interface Book {
  readonly id: BookId;
  readonly path: string;
  /** Canonical text and stamp, from whichever seat holds it. */
  source(): Source;
  /**
   * The one write path. `changes` are in the coordinates of the text BEFORE
   * the edit (CodeMirror's change-set convention) and must not overlap.
   * Synchronous: returns after every subscriber has run.
   */
  apply(
    changes: Change | readonly Change[],
    origin: Origin,
    trust?: Trust,
  ): Result.Result<Receipt, Refusal>;
  /** Synchronous fan-out in subscription order, after acceptance. */
  changes(fn: ChangeListener): () => void;
  /** CodeMirror history when instantiated; `null` for the plain Book. */
  history(): History | null;
}
export const asChangeList = (changes: Change | readonly Change[]): readonly Change[] =>
  // SAFETY: the parameter is a Change or a readonly Change[]; when it is not an
  // array it can only be the single Change.
  Array.isArray(changes) ? changes : [changes as Change];
/**
 * A small subscriber set shared by both Book implementations: snapshot the set
 * before iterating so a listener may unsubscribe itself, and never let one
 * listener's throw hide the edit from the rest.
 */
export const makeListeners = (): {
  add: (fn: ChangeListener) => () => void;
  publish: (receipt: Receipt, changes: readonly Change[]) => void;
} => {
  const set = new Set<ChangeListener>();
  return {
    add: (fn) => {
      set.add(fn);
      return () => {
        set.delete(fn);
      };
    },
    publish: (receipt, changes) => {
      for (const fn of Array.from(set)) fn(receipt, changes);
    },
  };
};
