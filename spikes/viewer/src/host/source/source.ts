// SPIKE: types-only stub of Sefer #core/source/source (HEAD 50aa9cd). The editor imports only types from here.
export interface SourceStamp {
  readonly revision: number;
  readonly length: number;
}

/**
 * How the file was written on disk, remembered so that Sefer can write it back
 * the same way.
 *
 * It is deliberately NOT part of the stamp and never part of any comparison:
 * the text a Book holds is always canonical (LF, no mark), and every identity
 * question — dirty, baseline, diff, external change — is asked of that text.
 * The form is a property of the BYTES, carried alongside so that `encode` can
 * reproduce them. Two Sources with the same text and different forms are the
 * same text.
 *
 * `eol` is the file's DOMINANT line ending, not a promise that the file was
 * uniform. A file that mixes styles is read as its majority and written back
 * uniform in that majority the first time it is saved.
 */
export interface SourceForm {
  readonly eol: "lf" | "crlf";
  /** Did the file begin with a UTF-8 byte order mark? */
  readonly bom: boolean;
}

export interface Source {
  readonly text: string;
  readonly stamp: SourceStamp;
  /** The disk form these bytes came in, and the one `encode` writes back. */
  readonly form: SourceForm;
}

export interface Change {
  readonly from: number;
  readonly to: number;
  readonly insert: string;
}
