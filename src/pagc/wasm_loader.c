/*
 * wasm_loader.c — wasm/standalone loader replacement for `std_pg_hash.c`.
 *
 * The upstream PAGC PostGIS extension loaded its lex/gaz/rules tables
 * out of PostgreSQL system tables (us_lex, us_gaz, us_rules). For the
 * wasm component we ship the same data as constant byte buffers (the
 * SQL files in `data/`, embedded by the Rust crate via include_bytes!),
 * and parse the embedded VALUES tuples here without a SQL engine.
 *
 * The parser is intentionally narrow: it accepts only the syntactic
 * shapes the vendored 13_us_lex.sql / 14_us_gaz.sql / 15_us_rules.sql
 * actually use, namely:
 *
 *   INSERT INTO us_lex (seq, word, stdword, token)
 *   WITH t(seq,word,stdword,token) AS ( VALUES (1, '#', '#', 16),
 *                                             (2, '#', '#',  7), ... );
 *
 *   INSERT INTO us_rules (rule) VALUES ('1 -1 5 -1 2 7');
 *
 * Anything else is silently skipped. Quoted strings honour SQL-style
 * ''-escaping for embedded single quotes.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>

#include "pagc_api.h"
#include "pagc_std_api.h"

/* -- Lex/Gaz tuple shape: (seq INT, word TEXT, stdword TEXT, token INT) -- */

/* Forward decl from lexicon.c / gamma.c. */
extern LEXICON *lex_init(ERR_PARAM *err_p);
extern int lex_add_entry(LEXICON *lex, int seq, char *word, char *stdword, SYMB token);

extern RULES *rules_init(ERR_PARAM *err_p);
extern int rules_add_rule(RULES *rules, int num, int *rule);
extern int rules_ready(RULES *rules);

/* PAGC's pg_hash loader allows rules of at most MAX_RULE_LENGTH ints
 * (defined in pagc_common.h via pagc_api.h). 200 is generous enough for
 * the vendored 15_us_rules.sql which tops out around ~25 ints/rule. */
#ifndef MAX_RULE_LENGTH
#define MAX_RULE_LENGTH 200
#endif

/* parse_rule: lifted verbatim from std_pg_hash.c's parse_rule(). Reads
 * a space-separated decimal sequence into an int array, returning the
 * count or -1 on overflow. */
static int
parse_rule(char *buf, int *rule)
{
    int nr = 0;
    int *r = rule;
    char *p = buf;
    char *q;

    while (1) {
        if (nr >= MAX_RULE_LENGTH) return -1;
        *r = strtol(p, &q, 10);
        if (p == q) break;
        p = q;
        nr++;
        r++;
    }
    return nr;
}

/* Local replacement for the never-defined upstream rules_add_rule_from_str. */
static int
add_rule_from_str(RULES *rules, char *rule_str)
{
    int rule_arr[MAX_RULE_LENGTH];
    int nr = parse_rule(rule_str, rule_arr);
    if (nr <= 0) return -1;
    return rules_add_rule(rules, nr, rule_arr);
}

extern STANDARDIZER *std_init(void);
extern int std_use_lex(STANDARDIZER *std, LEXICON *lex);
extern int std_use_gaz(STANDARDIZER *std, LEXICON *gaz);
extern int std_use_rules(STANDARDIZER *std, RULES *rules);
extern int std_ready_standardizer(STANDARDIZER *std);


static const char *
skip_ws(const char *p, const char *end)
{
    while (p < end && (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r'))
        p++;
    return p;
}

/* Parse one single-quoted SQL string starting at *p (which points to '\''),
 * advancing *p past the closing quote. Writes a malloc'd NUL-terminated
 * copy into *out (caller frees). Handles '' as escaped single quote.
 * Returns 0 on success, -1 on parse failure. */
static int
parse_sql_string(const char **p, const char *end, char **out)
{
    if (*p >= end || **p != '\'')
        return -1;
    (*p)++;

    size_t cap = 32, len = 0;
    char *buf = (char *) malloc(cap);
    if (!buf) return -1;

    while (*p < end) {
        char c = **p;
        if (c == '\'') {
            /* '' is an escaped single quote; closing quote otherwise. */
            if (*p + 1 < end && (*p)[1] == '\'') {
                if (len + 1 >= cap) { cap *= 2; buf = (char *) realloc(buf, cap); }
                buf[len++] = '\'';
                *p += 2;
                continue;
            }
            (*p)++;
            buf[len] = '\0';
            *out = buf;
            return 0;
        }
        if (len + 1 >= cap) { cap *= 2; buf = (char *) realloc(buf, cap); }
        buf[len++] = c;
        (*p)++;
    }
    free(buf);
    return -1;
}

/* Parse a positive integer or -1 (rules use -1 as a separator token). */
static int
parse_int(const char **p, const char *end, int *out)
{
    *p = skip_ws(*p, end);
    int sign = 1;
    if (*p < end && **p == '-') { sign = -1; (*p)++; }
    if (*p >= end || !isdigit((unsigned char) **p))
        return -1;
    int v = 0;
    while (*p < end && isdigit((unsigned char) **p)) {
        v = v * 10 + (**p - '0');
        (*p)++;
    }
    *out = sign * v;
    return 0;
}

/* Parse one VALUES tuple of form (int, 'str', 'str', int) starting at the
 * '(' character. On success, advances *p past the ')' and fills the four
 * components. *word and *stdword are heap-allocated; caller must free. */
static int
parse_lex_tuple(const char **p, const char *end,
                int *seq, char **word, char **stdword, int *token)
{
    *p = skip_ws(*p, end);
    if (*p >= end || **p != '(') return -1;
    (*p)++;

    if (parse_int(p, end, seq) < 0) return -1;
    *p = skip_ws(*p, end);
    if (*p >= end || **p != ',') return -1;
    (*p)++;
    *p = skip_ws(*p, end);

    if (parse_sql_string(p, end, word) < 0) return -1;
    *p = skip_ws(*p, end);
    if (*p >= end || **p != ',') { free(*word); *word = NULL; return -1; }
    (*p)++;
    *p = skip_ws(*p, end);

    if (parse_sql_string(p, end, stdword) < 0) { free(*word); *word = NULL; return -1; }
    *p = skip_ws(*p, end);
    if (*p >= end || **p != ',') { free(*word); free(*stdword); return -1; }
    (*p)++;

    if (parse_int(p, end, token) < 0) { free(*word); free(*stdword); return -1; }
    *p = skip_ws(*p, end);
    if (*p >= end || **p != ')') { free(*word); free(*stdword); return -1; }
    (*p)++;
    return 0;
}

/* Walk a buffer of vendored 13_us_lex.sql or 14_us_gaz.sql, pushing each
 * tuple via lex_add_entry. Returns count of tuples installed. */
int
pagc_load_lex_from_sql(LEXICON *lex, const char *data, size_t len)
{
    const char *p = data;
    const char *end = data + len;
    int count = 0;

    while (p < end) {
        /* Find the next '(' opening a tuple. Skip anything else. */
        if (*p != '(') { p++; continue; }

        int seq = 0, token = 0;
        char *word = NULL, *stdword = NULL;
        const char *start = p;
        if (parse_lex_tuple(&p, end, &seq, &word, &stdword, &token) < 0) {
            /* Not a tuple we recognise; skip the '(' and continue. */
            p = start + 1;
            continue;
        }

        if (lex_add_entry(lex, seq, word, stdword, (SYMB) token))
            count++;

        free(word);
        free(stdword);
    }
    return count;
}

/* Walk a buffer of vendored 15_us_rules.sql, pushing each rule string via
 * rules_add_rule_from_str. Returns count of rules installed. */
int
pagc_load_rules_from_sql(RULES *rules, const char *data, size_t len)
{
    const char *p = data;
    const char *end = data + len;
    int count = 0;

    while (p < end) {
        /* Look for `VALUES (` then a single-quoted string. */
        if (*p != '\'') { p++; continue; }

        const char *start = p;
        char *rule = NULL;
        if (parse_sql_string(&p, end, &rule) < 0) {
            p = start + 1;
            continue;
        }
        if (rule && rule[0] != '\0') {
            if (add_rule_from_str(rules, rule) == 0)
                count++;
        }
        free(rule);
    }
    return count;
}

/* High-level convenience: build a STANDARDIZER from the three SQL buffers
 * the wasm component ships in via include_bytes!. Returns NULL on failure. */
STANDARDIZER *
pagc_build_standardizer(const char *lex_sql, size_t lex_len,
                        const char *gaz_sql, size_t gaz_len,
                        const char *rules_sql, size_t rules_len)
{
    LEXICON *lex = lex_init(NULL);
    if (!lex) return NULL;
    pagc_load_lex_from_sql(lex, lex_sql, lex_len);

    LEXICON *gaz = lex_init(NULL);
    if (!gaz) return NULL;
    pagc_load_lex_from_sql(gaz, gaz_sql, gaz_len);

    RULES *rules = rules_init(NULL);
    if (!rules) return NULL;
    pagc_load_rules_from_sql(rules, rules_sql, rules_len);
    if (rules_ready(rules) != 0) return NULL;

    STANDARDIZER *std = std_init();
    if (!std) return NULL;

    /* std_use_lex follows TRUE/FALSE convention (TRUE=success), while
     * std_use_gaz/std_use_rules/std_ready_standardizer follow C
     * 0=success convention. Mirror upstream std_pg_hash.c handling. */
    if (std_use_lex(std, lex) == 0 /* FALSE */) return NULL;
    if (std_use_gaz(std, gaz) != 0) return NULL;
    if (std_use_rules(std, rules) != 0) return NULL;
    if (std_ready_standardizer(std) != 0) return NULL;

    return std;
}
