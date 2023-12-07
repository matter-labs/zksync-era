# ZK Terminology

### Arithmetization

Arithmetization refers to a technique used in zero-knowledge proof systems, where computations are represented in such a way that they can be efficiently verified by a prover and a verifier. In simpler terms, it is a way to convert computations into polynomial equations so that they can be used in cryptographic proofs.

### Builder

The builder helps set up the constraint system. The builder knows the placement strategy for each gate, as well as the geometry and other information needed for building the constraint system.

### Circuit

An arithmetic circuit is a mathematical construct used in cryptography to encode a computational problem. It is comprised of gates, with each gate performing an arithmetic operation, for example, such as addition or multiplication. These circuits are essential for encoding statements or computations that a prover wants to prove knowledge of without revealing the actual information.

### Constraint

A constraint is a rule or restriction that a specific operation or set of operations must follow. zkSync uses constraints to verify the validity of certain operations, and in the generation of proofs. Constraints can be missing, causing bugs, or there could be too many constraints, leading to restricted operations.

### Constraint degree

The "constraint degree" of a constraint system refers to the maximum degree of the polynomial gates in the system. In simpler terms, it’s the highest power of polynomial equations of the constraint system. At zkSync, we allow gates with degree 8 or lower.

### Constraint system

Constraint system is a mathematical representation consisting of a set of equations or polynomial constraints that are constructed to represent the statements being proved. The system is deemed satisfied if there exist specific assignments to the variables that make all the equations or constraints true. Imagine it as a list of "placeholders" called Variables. Then we add gates to the variables which enforce a specific constraint. The Witness represents a specific assignment of values to these Variables, ensuring that the rules still hold true.

### Geometry

The geometry defines the number of rows and columns in the constraint system. As part of PLONK arithmetization, the witness data is arranged into a grid, where each row defines a gate (or a few gates), and the columns are as long as needed to hold all of the witness data. At zkSync, we have ~164 base witness columns.

### Log

We use the word “log” in the sense of a database log: a log stores a list of changes.

### Lookup table

Lookup table is a predefined table used to map input values to corresponding output values efficiently, assisting in validating certain relations or computations without revealing any extra information about the inputs or the internal computations. ****Lookup ****tables are particularly useful in ZK systems to optimize and reduce the complexity of computations and validations, enabling the prover to construct proofs more efficiently and the verifier to validate relationships or properties with minimal computational effort. For example, if you want to prove a certain number is between 0 and 2^8, it is common to use a lookup table.

### Proof

A proof can refer generally to the entire proving process, or a proof can refer specifically to the data sent from the prover to the verifier. 

### Prover

In our zkSync zk-rollup context, the prover is used to process a set of transactions executing smart contracts in a succinct and efficient manner. It computes proofs that all the transactions are correct and ensures a valid transition from one state to another. The proof will be sent to a Verifier smart contract on Ethereum. At zkSync, we prove state diffs of a block of transactions, in order to prove the new state root state is valid.

### Satisfiable

In the context of ZK, satisfiability refers to whether the witness passes - or “satisfies” - all of the constraints in a circuit. 

### State Diffs

State Diffs, or State Differentials, are the differences in accounts before and after processing transactions contained in a block. For example, if my ETH Balance changes from 5 ETH to 6 ETH, then the state diff for my account is +1 ETH.

### Variables

Variables are placeholders in the constraint system until we know the specific witness data. The reason we would want placeholders is because we might want to fill in which constraints we need, before knowing the actual input data. For example, we might know we need to add two numbers and constrain the sum, before we know exactly which two numbers will be in the witness.

### Verifier

The Verifier is a smart contract on Ethereum. It will receive a proof, check to make sure the proof is valid, and then update the state root.

### Witness

Witness refers to the private, secret information or set of values that the prover possesses and aims to demonstrate knowledge of, without revealing the actual information to the verifier. The witness is the input to the circuit. When we have a circuit, the valid “witness” is the input that meets all the constraints and satisfies everything.

### Worker

A worker refers to our multi-threaded proving system. Proving may be “worked” in parallel, meaning that we can execute some operations, like polynomial addition, in parallel threads.