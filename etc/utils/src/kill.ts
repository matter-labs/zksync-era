import { promisify } from "node:util";
import { exec } from "node:child_process";

export async function killPidWithAllChilds(pid: number, signalNumber: number) {
  let childs = [pid];
  while (true) {
    try {
      let child = childs.at(-1);
      childs.push(+(await promisify(exec)(`pgrep -P ${child}`)).stdout);
    } catch (e) {
      break;
    }
  }
  // We always run the test using additional tools, that means we have to kill not the main process, but the child process
  for (let i = childs.length - 1; i >= 0; i--) {
    try {
      await promisify(exec)(`kill -${signalNumber} ${childs[i]}`);
    } catch (e) {
      console.log(`Failed to kill ${childs[i]} with ${e}`);
    }
  }
}
