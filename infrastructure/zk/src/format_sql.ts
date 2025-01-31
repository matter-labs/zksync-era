import * as fs from 'fs';
import * as utils from 'utils';
import { format } from 'sql-formatter';

function formatQuery(query: string) {
    let formattedQuery = query;
    try {
        formattedQuery = format(query, {
            language: 'postgresql',
            tabWidth: 4,
            keywordCase: 'upper',
            expressionWidth: 80,
            indentStyle: 'standard'
        });
    } catch {
        console.error(`Unable to format:\n${query}\n`);
    }

    // sql-formatter is not smart enough to identify whether something is used as a keyword or a column name
    const keywordToLower = ['STORAGE', 'TIMESTAMP', 'INPUT', 'DATA', 'ZONE', 'VALUE', 'DEPTH', 'KEY'];
    for (const keyword of keywordToLower) {
        // we replace keyword but only if it's not part of a longer word
        formattedQuery = formattedQuery.replace(RegExp(`\\b${keyword}\\b`, 'g'), keyword.toLowerCase());
    }

    const formattedLines = formattedQuery.split('\n');

    const minIndent = Math.min(...formattedLines.map((line) => line.search(/\S/)));
    formattedQuery = formattedQuery
        .split('\n')
        .map((line) => line.slice(minIndent))
        .join('\n');

    return formattedQuery;
}

function extractQueryFromRustString(query: string, isRaw: boolean): string {
    query = query.trim();
    if (query.endsWith(',')) {
        query = query.slice(0, query.length - 1);
    }
    //removing quotes
    if (!query.startsWith('r#')) {
        query = query.slice(1, query.length - 1);
    } else {
        query = query.slice(3, query.length - 2);
    }

    // Get rid of all "\" characters, both from escapes and line breaks.
    if (!isRaw) {
        query = query.replace(/\\(.|\n)/g, '$1');
    }
    return query;
}

function embedTextInsideRustString(query: string) {
    return 'r#"\n' + query + '\n"#';
}

function addIndent(query: string, indent: number) {
    return query
        .split('\n')
        .map((line) => ' '.repeat(indent) + line)
        .join('\n');
}

function formatRustStringQuery(query: string, isRaw: boolean) {
    const baseIndent = query.search(/\S/);
    const rawQuery = extractQueryFromRustString(query, isRaw);
    const formattedQuery = formatQuery(rawQuery);
    const reconstructedRustString = embedTextInsideRustString(formattedQuery);

    return addIndent(reconstructedRustString, baseIndent);
}

function formatOneLineQuery(line: string): string {
    const isRawString = line.includes('sqlx::query!(r');
    const queryStart = isRawString ? line.indexOf('r#"') : line.indexOf('"');
    const baseIndent = line.search(/\S/) + 4;
    const prefix = line.slice(0, queryStart);
    line = line.slice(queryStart);
    const queryEnd = isRawString ? line.indexOf('"#') + 2 : line.slice(1).search(/(^|[^\\])"/) + 3;
    const suffix = line.slice(queryEnd);
    const query = line.slice(0, queryEnd);
    let formattedQuery = formatRustStringQuery(query, isRawString);
    formattedQuery = addIndent(formattedQuery, baseIndent);

    return prefix + '\n' + formattedQuery + '\n' + suffix;
}

async function formatFile(filePath: string, check: boolean) {
    const content = await fs.promises.readFile(filePath, { encoding: 'utf-8' });
    let linesToQuery = null;
    let isInsideQuery = false;
    let isRawString = false;
    let builtQuery = '';

    let modifiedFile = '';
    for (const line of content.split('\n')) {
        if (line.includes('sqlx::query!("') || line.includes('sqlx::query!(r')) {
            modifiedFile += formatOneLineQuery(line) + '\n';
            continue;
        }

        if (line.endsWith('sqlx::query!(')) {
            linesToQuery = 1;
            isRawString = false;
            builtQuery = '';
        }
        if (line.endsWith('sqlx::query_as!(')) {
            linesToQuery = 2;
            isRawString = false;
            builtQuery = '';
        }

        if (linesToQuery !== null) {
            if (linesToQuery === 0) {
                isInsideQuery = true;
                linesToQuery = null;
                if (line.includes('r#"')) {
                    isRawString = true;
                }
            } else {
                linesToQuery -= 1;
            }
        }

        if (isInsideQuery) {
            const queryNotEmpty = builtQuery !== '' || line.trim().length > 1;
            const rawStringQueryEnded = line.endsWith('"#,') || line.endsWith('"#');
            const regularStringQueryEnded = (line.endsWith('",') || line.endsWith('"')) && queryNotEmpty;
            builtQuery += line + '\n';
            const lineEndIsNotEscape = !line.endsWith('\\"') && !line.endsWith('\\",');
            if (
                (isRawString && rawStringQueryEnded) ||
                (!isRawString && regularStringQueryEnded && lineEndIsNotEscape)
            ) {
                isInsideQuery = false;
                let endedWithComma = builtQuery.trimEnd().endsWith(',');
                modifiedFile += formatRustStringQuery(builtQuery, isRawString).trimEnd();
                modifiedFile += endedWithComma ? ',' : '';
                modifiedFile += '\n';
            }
        } else {
            modifiedFile += line + '\n';
        }
    }
    modifiedFile = modifiedFile.slice(0, modifiedFile.length - 1);
    if (content !== modifiedFile) {
        if (check) {
            console.warn(`Sqlx query format issues found in ${filePath}.`);
        } else {
            await fs.promises.writeFile(filePath, modifiedFile);
        }
    }
    return content === modifiedFile;
}

export async function formatSqlxQueries(check: boolean) {
    process.chdir(`${process.env.ZKSYNC_HOME}`);
    const { stdout: filesRaw } = await utils.exec(
        'find core/lib/dal -type f -name "*.rs" && find prover/crates/lib/prover_dal -type f -name "*.rs"'
    );
    const files = filesRaw.trim().split('\n');
    const formatResults = await Promise.all(files.map((file) => formatFile(file, check)));
    if (check) {
        if (!formatResults.includes(false)) {
            console.log('All files contain correctly formatted sqlx queries!');
        } else {
            throw new Error('Some files contain incorrectly formatted sql queries');
        }
    }
}
