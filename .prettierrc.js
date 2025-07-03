module.exports = {
    // Common settings
    printWidth: 120,
    singleQuote: true,
    trailingComma: 'none',
    bracketSpacing: true,
    tabWidth: 4,

    overrides: [
        // TypeScript/TSX files
        {
            files: ['*.ts', '*.tsx'],
            options: {
                parser: 'typescript'
            }
        },
        // Markdown files
        {
            files: ['*.md'],
            options: {
                tabWidth: 2,
                parser: 'markdown',
                proseWrap: 'always'
            }
        }
    ]
};
