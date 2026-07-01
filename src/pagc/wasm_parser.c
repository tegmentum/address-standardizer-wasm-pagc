/*
 * wasm_parser.c - wasm/standalone glue for PAGC's parseaddress() path.
 *
 * The upstream PAGC PostGIS extension exposes `parse_address()` as a
 * SRF wrapper (address_parser.c) that constructs a PG_FUNCTION_ARGS
 * tuple. In the wasm component we bypass the PG SRF layer and call
 * `parseaddress()` directly, letting the safe Rust wrapper in
 * `src/ops.rs` marshal the resulting ADDRESS struct into a
 * `StandardizedAddress`.
 *
 * The state hash is built once per component instance and reused: PAGC's
 * `load_state_hash()` populates it from a static ~110-entry state/name
 * lookup used inside `parseaddress()`.
 */

#include <stdlib.h>
#include <string.h>

#include "postgres.h"
#include "parseaddress-api.h"

/* Lazy singleton state hash. parseaddress() only ever reads it, so it is
 * safe to share across calls within a single wasm instance. */
static HHash *g_state_hash = NULL;

static int
ensure_state_hash(void)
{
    if (g_state_hash)
        return 0;

    HHash *h = hash_new();
    if (!h)
        return 1001;

    int err = load_state_hash(h);
    if (err) {
        hash_free(h);
        return err;
    }

    g_state_hash = h;
    return 0;
}

/*
 * Parse an address string. `input` is copied internally (parseaddress()
 * writes into its input buffer). The returned ADDRESS is heap-allocated
 * with fields owned by the caller; free via `pagc_address_free`.
 *
 * Returns NULL on OOM or state-hash bootstrap failure. On regex miss
 * PAGC returns a zero-initialised ADDRESS (all NULL fields) rather than
 * NULL, so callers should not treat a partial result as a failure.
 */
ADDRESS *
pagc_parse_address(const char *input)
{
    if (ensure_state_hash() != 0)
        return NULL;

    if (!input)
        input = "";

    /* parseaddress modifies its buffer in place; use a private copy. */
    char *buf = strdup(input);
    if (!buf)
        return NULL;

    int err = 0;
    ADDRESS *a = parseaddress(g_state_hash, buf, &err);

    free(buf);
    return a;
}

/*
 * Free an ADDRESS returned by `pagc_parse_address`. All string fields
 * come from palloc0/strdup (backed by libc malloc via postgres.h shim)
 * so free() reclaims them.
 */
void
pagc_address_free(ADDRESS *a)
{
    if (!a)
        return;
    free(a->num);
    free(a->street);
    free(a->street2);
    free(a->address1);
    free(a->city);
    free(a->st);
    free(a->zip);
    free(a->zipplus);
    free(a->cc);
    free(a);
}
