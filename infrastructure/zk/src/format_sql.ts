import * as fs from 'fs';
import path from 'path';
import * as utils from './utils';
import {format} from "sql-formatter";
import {max} from "hardhat/internal/util/bigint";


function formatQuery(query: string) {
    let formattedQuery = query;
    try {
        formattedQuery = format(query,{
            language: 'postgresql',
            tabWidth: 4,
            keywordCase: 'upper',
            expressionWidth: 80,
            indentStyle: 'tabularRight',
        })
    } catch {
        console.log(`Unable to format:\n${query}\n`)
    }

    // sql-formatter is not smart enough to identify whether something is used as a keyword or a column name
    const keywordToLower = ['STORAGE','TIMESTAMP', 'INPUT','DATA', 'ZONE', 'VALUE', 'DEPTH'];
    for (const keyword of keywordToLower) {
        // we replace keyword but only if it's not part of a longer word
        formattedQuery = formattedQuery.replace(`/\\b${keyword}\\b/g`, keyword.toLowerCase());
    }

    const formattedLines = formattedQuery.split('\n')

    const minIndent = Math.min(...formattedLines.map(line => line.search(/\S/)));
    formattedQuery = formattedQuery.split("\n").map(line => line.slice(minIndent)).join("\n");

    return formattedQuery;
}
function formatRustStringQuery(baseIdent: number, isRawString: boolean, query: string) {
    query = query.trim();
    let endedWithAComma = query.endsWith(',')
    if (query.endsWith(',')) {
        query = query.slice(0, query.length - 1);
    }
    if (!isRawString) {
        query = query.slice(1, query.length - 1);
    } else {
        query = query.slice(3, query.length - 2);
    }

    query = query
        .trim()
        .split('\n')
        .map(line => line.endsWith('\\') ? line.slice(0, line.length - 1) : line)
        .join('\n');

    while (!isRawString && query.includes('\\"')) {
        query = query.replace('\\"', '"');
    }

    let formattedQuery = formatQuery(query);

    const indent = ' '.repeat(baseIdent);
    formattedQuery = formattedQuery.replace(/"/g, '\\"');
    const formattedLines = formattedQuery.split('\n')
    // sql-formatter is not smart enough to identify whether something is used as a keyword or a column name

    const minIndent = Math.min(...formattedLines.map(line => line.search(/\S/)));

    for (let i =0;i<formattedLines.length;i++) {
        let currentLine  = indent;
        currentLine += i == 0 ? '"' : ' ';
        currentLine += formattedLines[i].slice(minIndent);
        currentLine += i != formattedLines.length - 1 ? ' \\' : '';
        currentLine += i == formattedLines.length - 1 ? '"' : '';
        currentLine += i == formattedLines.length - 1 && endedWithAComma ? ',' : '';

        formattedLines[i] = currentLine;
    }
    return formattedLines.join('\n') + '\n';
}

async function formatFile(filePath: string) {
    let content = fs.readFileSync(filePath, {encoding: 'utf-8'}).split('\n');
    let linesToQuery = null;
    let isInsideQuery = false;
    let isRawString = false;
    let builtQuery = '';
    let baseIdent = 0;
    let queriesCount = 0;


    let modifiedFile = ''
    for (const line of content) {
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
                baseIdent = line.search(/\S/);
            } else {
                linesToQuery -= 1;
            }
        }

        if (isInsideQuery) {
            builtQuery += line.trim() + '\n';

            const rawStringQueryEnded = (line.endsWith('"#,') || line.endsWith('"#')) && builtQuery.length != 3;
            const regularStringQueryEnded = (line.endsWith('",') || line.endsWith('"')) && builtQuery.length != 2;
            const lineEndIsNotEscape = !line.endsWith('\\"') && !line.endsWith('\\",');
            if (
                (isRawString && rawStringQueryEnded) ||
                (!isRawString && regularStringQueryEnded && lineEndIsNotEscape)
            ) {
                isInsideQuery = false;
                let formattedQuery = formatRustStringQuery(baseIdent, isRawString, builtQuery);
                modifiedFile += formattedQuery;
                queriesCount += 1;
            }
        } else {
            modifiedFile += line + '\n';
        }
    }
    fs.writeFileSync(filePath, modifiedFile.slice(0, modifiedFile.length - 1));
    return queriesCount;
}

export async function formatSqlxQueries() {
    process.chdir(`${process.env.ZKSYNC_HOME}`);
    let {stdout: filesRaw} = await utils.exec('find core/lib/dal -type f -name "*.rs"');
    let files = filesRaw.trim().split('\n')
    let processedQueries = 0
    for (let file of files) {
        processedQueries += await formatFile(file);
    }
    console.log(`Successfully formatted ${processedQueries} queries in ${files.length} files!`)
}
