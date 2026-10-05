/**
 * The placeholder id baked into statically exported dynamic routes.
 *
 * A dynamic route like `/jobs/[id]` must be exported as *something*, and the
 * real ids are UUIDs that only exist in the database. This value is never used to
 * fetch anything: each view reads the id from the URL at runtime, and the API
 * supplies the entity.
 *
 * It exists so the export produces one shell per route, which the server falls
 * back to for any unmatched path. A pasted deep link therefore boots the console
 * rather than 404ing.
 *
 * It is deliberately not a valid UUID, so if it ever escaped into a request the
 * server would reject it loudly rather than quietly looking up nothing.
 */
export const EXPORT_ID = "__static_export__";