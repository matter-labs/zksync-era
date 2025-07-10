export function fail(_: any, message: string) {
  return {
    pass: false,
    message: () => (message ? message : "fails by .fail() assertion"),
  };
}
