import process from 'node:process';

if (!process.env.LOTAR_TEST_SILENT) {
    process.env.LOTAR_TEST_SILENT = '1';
}

if (!process.env.LOTAR_BINARY_PATH) {
    // Allow overriding via LOTAR_BIN for backwards compatibility
    if (process.env.LOTAR_BIN) {
        process.env.LOTAR_BINARY_PATH = process.env.LOTAR_BIN;
    }
}
