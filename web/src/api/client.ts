// The one Effect service: the hub's /v1 API over Effect's HttpClient.
//
// - Responses are typed from the generated OpenAPI paths. `decode` runs a
//   Schema over a body instead, for a response the page must not trust blindly.
// - Every failure (network, status, decode) becomes one tagged `ApiError`.
// - Every request carries a W3C `traceparent` (one trace id per page view, a
//   new span id per call) and `x-hub-page`, so the server's wide events for a
//   page view can be grouped. That is all the observability the browser does.
import { Context, Data, Effect, Layer, ManagedRuntime, Schema } from "effect";
import { FetchHttpClient, HttpClient, HttpClientRequest, type HttpClientResponse } from "effect/unstable/http";
import type { paths } from "./schema";

export class ApiError extends Data.TaggedError("ApiError")<{
  readonly url: string;
  /** HTTP status, or null when no response arrived. */
  readonly status: number | null;
  readonly message: string;
}> {}

// --- page views ---------------------------------------------------------------

const hex = (bytes: number) =>
  Array.from(crypto.getRandomValues(new Uint8Array(bytes)), (b) => b.toString(16).padStart(2, "0")).join("");

let view = { page: "", traceId: hex(16) };

/** Called by each page when it mounts: a new page view, a new trace id. */
export const startPageView = (page: string): void => {
  view = { page, traceId: hex(16) };
};

// --- typed paths --------------------------------------------------------------

type Get<P extends keyof paths> = paths[P] extends { get: infer G } ? G : never;
type PathParams<P extends keyof paths> = Get<P> extends { parameters: { path: infer A } } ? A : never;
type QueryParams<P extends keyof paths> = Get<P> extends { parameters: { query?: infer Q } } ? Q : never;
/** The paths whose 200 response OpenAPI describes as JSON. */
export type JsonPath = {
  [P in keyof paths]: Get<P> extends { responses: { 200: { content: { "application/json": unknown } } } } ? P : never;
}[keyof paths];
export type JsonOf<P extends JsonPath> =
  Get<P> extends { responses: { 200: { content: { "application/json": infer J } } } } ? J : never;

export interface Call<P extends keyof paths> {
  readonly path?: PathParams<P>;
  readonly query?: QueryParams<P>;
}

/** `/v1/repos/{owner}/{name}` + `{owner, name}` → a URL. `{path}` keeps its slashes. */
export const urlOf = <P extends keyof paths>(template: P, call: Call<P> = {}): string => {
  const params = (call.path ?? {}) as Record<string, string>;
  const path = template.replace(/\{(\w+)\}/g, (_, k: string) =>
    String(params[k]).split("/").map(encodeURIComponent).join("/"),
  );
  const query = new URLSearchParams();
  for (const [k, v] of Object.entries((call.query ?? {}) as Record<string, unknown>))
    if (v !== undefined && v !== null && v !== "") query.set(k, String(v));
  return query.size ? `${path}?${query}` : path;
};

// --- the service --------------------------------------------------------------

export class Api extends Context.Service<
  Api,
  {
    readonly json: <P extends JsonPath>(path: P, call?: Call<P>) => Effect.Effect<JsonOf<P>, ApiError>;
    readonly decode: <P extends keyof paths, A>(
      path: P,
      schema: Schema.Codec<A, unknown>,
      call?: Call<P>,
    ) => Effect.Effect<A, ApiError>;
    readonly text: <P extends keyof paths>(path: P, call?: Call<P>) => Effect.Effect<string, ApiError>;
  }
>()("Api") {}

export const ApiLive = Layer.effect(
  Api,
  Effect.gen(function* () {
    const client = (yield* HttpClient.HttpClient).pipe(
      HttpClient.mapRequest((req) =>
        HttpClientRequest.setHeaders(req, {
          traceparent: `00-${view.traceId}-${hex(8)}-01`,
          "x-hub-page": view.page,
        }),
      ),
    );

    const send = (url: string) =>
      client.get(url).pipe(
        // The headers above are the whole of the browser's tracing: no client
        // spans, and no Effect-generated trace headers beside ours.
        Effect.provideService(HttpClient.TracerDisabledWhen, () => true),
        Effect.flatMap((res) =>
          res.status >= 200 && res.status < 300 ? Effect.succeed(res) : Effect.flatMap(errorText(res), (m) => fail(url, res.status, m)),
        ),
        Effect.catchTag("HttpClientError", (e) => fail(url, null, e.message)),
      );

    const body = <A>(url: string, read: (r: HttpClientResponse.HttpClientResponse) => Effect.Effect<A, unknown>) =>
      send(url).pipe(
        Effect.flatMap(read),
        Effect.mapError((e) => (e instanceof ApiError ? e : new ApiError({ url, status: null, message: String(e) }))),
      );

    return {
      json: (path, call) => {
        const url = urlOf(path, call);
        return body(url, (r) => r.json) as Effect.Effect<never, ApiError>;
      },
      decode: (path, schema, call) => {
        const url = urlOf(path, call);
        return body(url, (r) => Effect.flatMap(r.json, Schema.decodeUnknownEffect(schema)));
      },
      text: (path, call) => {
        const url = urlOf(path, call);
        return body(url, (r) => r.text);
      },
    };
  }),
).pipe(Layer.provide(FetchHttpClient.layer));

const fail = (url: string, status: number | null, message: string) => Effect.fail(new ApiError({ url, status, message }));

/** The server's `{"error": "…"}`, or the status line. */
const errorText = (res: HttpClientResponse.HttpClientResponse) =>
  res.json.pipe(
    Effect.map((b) => (b && typeof b === "object" && "error" in b ? String(b.error) : `HTTP ${res.status}`)),
    Effect.orElseSucceed(() => `HTTP ${res.status}`),
  );

const runtime = ManagedRuntime.make(ApiLive);

/** Run an Effect that needs the Api, for a Solid resource. Rejects with the ApiError. */
export const run = <A>(effect: Effect.Effect<A, ApiError, Api>): Promise<A> => runtime.runPromise(effect);

/** Promise shorthands for pages: one call, one Effect, run on the shared runtime. */
export const api = {
  json: <P extends JsonPath>(path: P, call?: Call<P>) =>
    run(Effect.gen(function* () {
      return yield* (yield* Api).json(path, call);
    })),
  decode: <P extends keyof paths, A>(path: P, schema: Schema.Codec<A, unknown>, call?: Call<P>) =>
    run(Effect.gen(function* () {
      return yield* (yield* Api).decode(path, schema, call);
    })),
  text: <P extends keyof paths>(path: P, call?: Call<P>) =>
    run(Effect.gen(function* () {
      return yield* (yield* Api).text(path, call);
    })),
};
