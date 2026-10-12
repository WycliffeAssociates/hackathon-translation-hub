// Stripped from workTracker shared/biel-duplicate-chapters-primary.ts (types only).
// The rule lives there and in biel-frontend; do not edit here — re-strip.
// Single source of truth for the biel duplicate-chapters (primary) report. Imported by
// BOTH the Astro site (the data-inconsistencies note + its API route) and the ops
// worker, so the collation rules live in exactly one place. Framework-agnostic:
// only fetch + Web Crypto, both available in the browser and in workerd.
//
// Keep in sync with biel-frontend src/data/pubDataApi.ts (doCollateContent /
// isKnownGatewayContentFormatException / collateGatewayContent).

export const ENDPOINTS = {
  prod: "https://api.bibleineverylanguage.org/v1/graphql",
  dev: "https://api-biel-dev.walink.org/v1/graphql",
}         ;
                                                 

// The prod biel site, for "see in biel" deep links (resource-type is matched
// case-insensitively there, so the raw resource_type is fine).
const BIEL_SITE = "https://bibleineverylanguage.org";
export const bielBookUrl = (ietf        , resourceType        , slug        ) =>
  `${BIEL_SITE}/resources/languages/${ietf}?resource-type=${encodeURIComponent(resourceType)}&book=${slug}&chapter=1`;

// Languages biel collates: every non-gateway language, plus these gateway exceptions.
const GATEWAY_COLLATE_EXCEPTIONS = ["ceb"];
const doCollate = (isGateway         , ietf        ) =>
  !isGateway || GATEWAY_COLLATE_EXCEPTIONS.includes(ietf);

// Only these domains get stitched (book/chapter schema), so only they can produce a
// visible duplicate book.
const STITCHED_DOMAINS = ["scripture", "gloss", "parascriptural"];

export const QUERY = `query DataInconsistencies {
  language(
    where: {contents: {wa_content_metadata: {status: {_eq: "Primary"}, show_on_biel: {_eq: true}}}}
  ) {
    ietf_code
    national_name
    wa_language_metadata { is_gateway }
    contents(
      where: {wa_content_metadata: {status: {_eq: "Primary"}, show_on_biel: {_eq: true}}, rendered_contents_aggregate: {count: {predicate: {_gt: 0}}}}
    ) {
      name
      type
      domain
      resource_type
      git_repo { repo_url }
      rendered_contents(
        where: {scriptural_rendering_metadata: {book_slug: {_is_null: false}}}
      ) {
        scriptural_rendering_metadata { book_slug }
      }
    }
  }
}`;

                          
               
               
                 
                        
                                        
                                                                                                   
 
                           
                    
                        
                                                       
                      
 
                         
               
                         
 
                                  
                    
                    
 
                           
                   
                
                                                    
 

// A stored snapshot of the report (what lands in KV and what the note renders).
                         
             
                        
                
                        
 

export function findConflicts(languages            )             {
  const conflicts             = [];
  for (const lang of languages) {
    const isGateway = lang.wa_language_metadata?.is_gateway ?? false;
    if (!doCollate(isGateway, lang.ietf_code)) continue;

    const groups = new Map                   ();
    for (const c of lang.contents) {
      const key = `${c.domain}-${c.resource_type}-${c.type}`;
      const arr = groups.get(key);
      if (arr) arr.push(c);
      else groups.set(key, [c]);
    }

    for (const [groupKey, contents] of groups) {
      const isStitched = STITCHED_DOMAINS.some((d) => groupKey.startsWith(`${d}-`));
      if (!isStitched || contents.length < 2) continue;

      const bySlug = new Map                  ();
      for (const c of contents) {
        const slugs = new Set(
          c.rendered_contents
            .map((r) => r.scriptural_rendering_metadata?.book_slug)
            .filter((s)              => !!s)
            .map((s) => s.toUpperCase()),
        );
        for (const slug of slugs) {
          const arr = bySlug.get(slug) ?? [];
          arr.push({ name: c.name, repoUrl: c.git_repo?.repo_url ?? null });
          bySlug.set(slug, arr);
        }
      }

      const resourceType = contents[0].resource_type;
      const conflictingBooks                                  = {};
      for (const [slug, sources] of bySlug) {
        if (sources.length > 1) {
          conflictingBooks[slug] = {
            seeInBiel: bielBookUrl(lang.ietf_code, resourceType, slug),
            sources,
          };
        }
      }
      if (Object.keys(conflictingBooks).length) {
        conflicts.push({
          language: `${lang.national_name} (${lang.ietf_code})`,
          group: groupKey,
          conflictingBooks,
        });
      }
    }
  }
  conflicts.sort((a, b) => a.language.localeCompare(b.language) || a.group.localeCompare(b.group));
  return conflicts;
}

// Fetches and collates. Throws on network / GraphQL error. Returns the raw response
// too, so the live view can offer "copy raw JSON".
export async function fetchConflicts(
  endpoint             ,
)                                                   {
  const res = await fetch(ENDPOINTS[endpoint], {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ query: QUERY }),
  });
  if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`);
  const json = (await res.json())     
                                    
                                        
   ;
  if (json.errors?.length) throw new Error(json.errors.map((e) => e.message).join("; "));
  if (!json.data) throw new Error("No data in response");
  return { conflicts: findConflicts(json.data.language), raw: json };
}

// Deterministic serialization of the full conflict set — this is what we hash, so it
// only changes when the actual conflicts do.
export function canonicalize(conflicts            )         {
  return [...conflicts]
    .sort((a, b) => a.language.localeCompare(b.language) || a.group.localeCompare(b.group))
    .map((c) => {
      const books = Object.keys(c.conflictingBooks)
        .sort()
        .map(
          (slug) =>
            `${slug}:${c.conflictingBooks[slug].sources
              .map((s) => s.name)
              .sort()
              .join(",")}`,
        )
        .join(";");
      return `${c.language}|${c.group}|${books}`;
    })
    .join("\n");
}

export async function sha256(input        )                  {
  const buf = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(input));
  return [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, "0")).join("");
}
