/*
 * Minimal PostgreSQL shim header for vendored PAGC core compiled
 * standalone (no libpq, no PG server). Provides only the macros and
 * memory helpers PAGC's portable sources actually reference.
 *
 * This header replaces the real `<postgres.h>` for the wasm/standalone
 * build. PAGC's `standard.c` is the only "portable" file that included
 * `postgres.h`; the deletions of std_pg_hash.c, address_standardizer.c
 * and address_parser.c remove the rest of the PG surface.
 */
#ifndef PAGC_POSTGRES_SHIM_H
#define PAGC_POSTGRES_SHIM_H

#include <stdlib.h>
#include <stdio.h>
#include <stdarg.h>
#include <stdbool.h>
#include <string.h>

/* PostgreSQL macro: array length. parseaddress-api.c uses it against
 * the static country_aliases[] table. */
#ifndef lengthof
#define lengthof(x) (sizeof(x) / sizeof((x)[0]))
#endif

/* Memory: PAGC's standard.c calls pfree() on heap strings produced by
 * its own allocator paths. We back palloc/pfree with malloc/free. */
#ifndef palloc
#define palloc(sz)   malloc(sz)
#endif
#ifndef palloc0
#define palloc0(sz)  calloc(1, (sz))
#endif
#ifndef pfree
#define pfree(p)     free(p)
#endif
#ifndef repalloc
#define repalloc(p, sz) realloc((p), (sz))
#endif

/* pstrdup: PG's strdup-from-current-memcontext. PAGC's standard.c uses
 * this to copy STDADDR field values into caller-owned strings. We just
 * forward to libc strdup since the wasm component manages its own
 * lifetime via stdaddr_free / free(). */
static inline char *
pagc_shim_pstrdup(const char *s)
{
    if (!s) return NULL;
    return strdup(s);
}
#ifndef pstrdup
#define pstrdup(s) pagc_shim_pstrdup(s)
#endif

/* Logging levels expected by PAGC's elog(NOTICE, ...) usage. */
#define DEBUG5  10
#define DEBUG4  11
#define DEBUG3  12
#define DEBUG2  13
#define DEBUG1  14
#define LOG     15
#define COMMERROR 16
#define INFO    17
#define NOTICE  18
#define WARNING 19
#define ERROR   20

/* elog: behave like a stderr printer. PAGC uses elog(NOTICE, ...) for
 * non-fatal diagnostics. */
static inline void
pagc_shim_elog(int level, const char *fmt, ...)
{
    (void)level;
    va_list ap;
    va_start(ap, fmt);
    vfprintf(stderr, fmt, ap);
    fputc('\n', stderr);
    va_end(ap);
}

#ifndef elog
#define elog(level, ...) pagc_shim_elog((level), __VA_ARGS__)
#endif

#endif /* PAGC_POSTGRES_SHIM_H */
