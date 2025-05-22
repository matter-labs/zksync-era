Object.defineProperty(BigInt.prototype, 'toJSON', {
    value() {
        return this.toString();
    }
});
