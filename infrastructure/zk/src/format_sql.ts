import * as fs from 'fs';
import * as utils from './utils';
import { format } from 'sql-formatter';

function formatQuery(query: string) {
    let formattedQuery = query;
    try {
        formattedQuery = format(query, {
            language: 'postgresql',
            tabWidth: 4,
            keywordCase: 'upper',
            expressionWidth: 80,
            indentStyle: 'tabularRight'
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

function extractQueryFromRustString(query: string): string {
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

    //getting rid of all "/" characters, both from escapes and line breaks
    query = query.replace(/\\/g, '');

    return query;
}

function embedTextInsideRustString(query: string) {
    query = query.replace(/"/g, '\\"');
    const formattedLines = query.split('\n');
    for (let i = 0; i < formattedLines.length; i++) {
        let currentLine = '';
        currentLine += i == 0 ? '"' : ' ';
        currentLine += formattedLines[i];
        currentLine += i != formattedLines.length - 1 ? ' \\' : '';
        currentLine += i == formattedLines.length - 1 ? '"' : '';

        formattedLines[i] = currentLine;
    }
    return formattedLines.join('\n');
}

function addIndent(query: string, indent: number) {
    return query
        .split('\n')
        .map((line) => ' '.repeat(indent) + line)
        .join('\n');
}

function formatRustStringQuery(query: string) {
    const baseIndent = query.search(/\S/);
    let endedWithAComma = query.trim().endsWith(',');

    const rawQuery = extractQueryFromRustString(query);
    const formattedQuery = formatQuery(rawQuery);
    const reconstructedRustString = embedTextInsideRustString(formattedQuery);

    return addIndent(reconstructedRustString, baseIndent) + (endedWithAComma ? ',' : '') + '\n';
}

async function formatFile(filePath: string, check: boolean) {
    let content = await fs.promises.readFile(filePath, { encoding: 'utf-8' });
    let linesToQuery = null;
    let isInsideQuery = false;
    let isRawString = false;
    let builtQuery = '';

    let modifiedFile = '';
    for (const line of content.split('\n')) {
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
            if (linesToQuery == 0) {
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
            const rawStringQueryEnded = (line.endsWith('"#,') || line.endsWith('"#')) && builtQuery;
            const regularStringQueryEnded = (line.endsWith('",') || line.endsWith('"')) && builtQuery;
            builtQuery += line + '\n';
            const lineEndIsNotEscape = !line.endsWith('\\"') && !line.endsWith('\\",');
            if (
                (isRawString && rawStringQueryEnded) ||
                (!isRawString && regularStringQueryEnded && lineEndIsNotEscape)
            ) {
                isInsideQuery = false;
                let formattedQuery = formatRustStringQuery(builtQuery);
                modifiedFile += formattedQuery;
            }
        } else {
            modifiedFile += line + '\n';
        }
    }
    modifiedFile = modifiedFile.slice(0, modifiedFile.length - 1);

    if (!check) {
        await fs.promises.writeFile(filePath, modifiedFile);
    } else {
        if (content !== modifiedFile) {
            console.warn(`Sqlx query format issues found in ${filePath}.`);
        }
    }
    return content === modifiedFile;
}

export async function formatSqlxQueries(check: boolean) {
    process.chdir(`${process.env.ZKSYNC_HOME}`);
    let { stdout: filesRaw } = await utils.exec('find core/lib/dal -type f -name "*.rs"');
    let files = filesRaw.trim().split('\n');
    let formatResults = await Promise.all(files.map((file) => formatFile(file, check)));
    if (check) {
        if (!formatResults.includes(false)) {
            console.log('All files contain correctly formatted sqlx queries!');
        } else {
            throw new Error('Some files contain incorrectly formatted sql queries');
        }
    }
}
