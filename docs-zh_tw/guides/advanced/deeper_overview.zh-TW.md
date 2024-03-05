 <!-- 翻譯時間：2024/3/5 -->
# 更深層概述(zk-SNARK)

[返回到specs目錄](../../specs/README.zh-TW.md)
[返回到根目錄](../../../README.zh-TW.md)

本部分的目的是從工程角度解釋我們的新證明系統。我們將檢查代碼範例以及庫之間的通信方式。

讓我們從討論我們的約束系統開始。在之前的證明器中，我們使用了 Bellman 存儲庫。然而，在新的證明器中，約束系統是在 Boojum 中實現的。

## 約束系統

如果您查看 boojum 存儲庫（src/cs/traits/cs.rs）：

```rust
pub trait ConstraintSystem<F: SmallField>: Send + Sync {

    ...
    fn alloc_variable() -> Variable;
    fn alloc_witness_without_value(&mut self) -> Witness;
    fn place_gate<G: Gate<F>>(&mut self, gate: &G, row: usize);
    ...
}
```

我們有三個主要部件：`Variable`、`Witness` 和 `Gate`。

為了理解約束系統，可以將其想象為一個名為 Variables 的預留列表。我們為這些 Variables 定義了規則，稱為“gates”。 Witness 表示對這些 Variables 值的具體分配，確保規則仍然成立。

在概念上，這與我們實現函數的方式類似。考慮以下示例：


```
fn fun(x) {
  y = x + A;
  z = y * B;
  w = if y { z } else { y }
}
```

在這個代碼片段中，`A`、`B`、`y`、`z` 和 `w` 是 Variables（其中 `A` 和 `B` 是常量）。我們建立了規則，或者說 gates，指定了變量 `z` 必須等於變量 `y` 乘以變量 `B`。

例如 Witness 分配可能是：

```
 x = 1; A = 3; y = 3; B = 0; z = 0; w = 3;
```

Gates 可以變得更加複雜。例如，`w` 案例展示了一個“選擇” gate，根據條件選擇兩個選項中的一個。

現在，讓我們深入研究這個 gate：

### 選擇 gate

代碼在 boojum/src/cs/gates/selection_gate.rs 中。

讓我們深入研究這個概念。我們的目標是創建一個 gate，實現邏輯 `result = if selector == true a else b;`。為了實現這一目標，我們將需要四個變量。

```rust
pub struct SelectionGate {
    pub a: Variable,
    pub b: Variable,
    pub selector: Variable,
    pub result: Variable,
}
```

內部 `Variable` 對象是 `pub struct Variable(pub(crate) u64);` - 因此它是約束系統對象中位置的索引。

現在讓我們看看如何將這個 gate 添加到系統中。

```rust
pub fn select<F: SmallField, CS: ConstraintSystem<F>>(
    cs: &mut CS,
    a: Variable,
    b: Variable,
    selector: Variable,
) -> Variable {
  //  First, let's allocate the output variable:
  let output_variable = cs.alloc_variable_without_value();
  ...
}
```

然後有一段見證者的代碼（暫時跳過它），最後一段代碼將 gate 添加到約束系統 `cs` 中：

```rust
    if <CS::Config as CSConfig>::SetupConfig::KEEP_SETUP {
        let gate = Self {
            a,
            b,
            selector,
            result: output_variable,
        };
        gate.add_to_cs(cs);
    }

    output_variable
```

總結一下，我們取了 3 個 'Variables'，創建了輸出變量，從中創建了一個 `SelectionGate` 對象，將其添加到系統中（通過調用 `add_to_cs`），最後返回輸出變量。

但是 'logic' 在哪裡？我們在哪裡實際強制執行約束？

為此，我們必須查看 `Evaluator`：

```rust
impl<F: SmallField> Gate<F> for SelectionGate {
    type Evaluator = SelectionGateConstraitEvaluator;

    #[inline]
    fn evaluator(&self) -> Self::Evaluator {
        SelectionGateConstraitEvaluator
    }
}
```

```rust
impl<F: PrimeField> GateConstraitEvaluator<F> for SelectionGateConstraitEvaluator {
  fn evaluate_once{
    let a = trace_source.get_variable_value(0);
    let b = trace_source.get_variable_value(1);
    let selector = trace_source.get_variable_value(2);
    let result = trace_source.get_variable_value(3);

    // contribution = a * selector
    let mut contribution = a;
    contribution.mul_assign(&selector, ctx);

    // tmp = 1 - selector
    let mut tmp = P::one(ctx);
    tmp.sub_assign(&selector, ctx);

    // contribution += tmp * b
    // So:
    // contribution = a*selector + (1-selector) * b
    P::mul_and_accumulate_into(&mut contribution, &tmp, &b, ctx);

    // contribution = a*selector + (1-selector) * b - result
    contribution.sub_assign(&result, ctx);

    // And if we're successful, the contribution == 0.
    // Because result == a * selector + (1-selector) * b
    destination.push_evaluation_result(contribution, ctx);
    }
}
```

這個評估器實際上是在 `Field` object 的基礎上操作，試圖構建和評估正確的多項式。其詳細內容將在另一篇文章中介紹。

恭喜，您現在應該理解了第一個 gate 的代碼。總結一下 - 我們創建了 'output' 變量，並將 gate 添加到 CS 系統中。稍後當 CS 系統 '計算' 所有依賴關係時，它將運行約束評估器，將原始的依賴關係（基本上是一個方程式）添加到列表中。

您可以查看 `src/cs/gates` 中的其他文件以查看其他示例。


## 結構

現在，我們已經處理了基本變數，讓我們看看我們可以用更複雜的結構做些什麼。Boojum 已經添加了一堆衍生巨集，讓開發更加輕鬆。

讓我們來看一個例子：

```rust
#[derive(Derivative, CSSelectable, CSAllocatable, CSVarLengthEncodable, WitnessHookable)]
pub struct VmLocalState<F: SmallField> {
    pub previous_code_word: UInt256<F>,
    pub registers: [VMRegister<F>; REGISTERS_COUNT],
    pub flags: ArithmeticFlagsPort<F>,
    pub timestamp: UInt32<F>,
    pub memory_page_counter: UInt32<F>,
```

首先，你在上面看到的所有 UInt，其實都是在 Boojum 中實現的：

```rust
pub struct UInt32<F: SmallField> {
    pub(crate) variable: Variable,
}
impl<F: SmallField> CSAllocatable<F> for UInt32<F> {
    // So the 'witness' type (concrete value) for U32 is u32 - no surprises ;-)
    type Witness = u32;
    ...
}

pub struct UInt256<F: SmallField> {
    pub inner: [UInt32<F>; 8],
}
```

### WitnessHookable

在上面的例子中，對於 U32 的 Witness 類型，是 u32 - 很簡單。但是當我們有更複雜的結構（如 VmLocalState）時，該怎麼辦呢？

這個衍生巨集會自動創建一個新的結構，命名為 XXWitness（在上面的例子中是 `VmLocalStateWitness`），可以填充具體的值。

### CsAllocatable

實現了 CsAllocatable - 這允許您直接在約束系統中「分配」這個結構（類似於我們在上面對常規「變數」進行操作的方式）。

### CSSelectable

實現了 `Selectable` 特性 - 允許該結構參與條件選擇等操作（因此它可以像上面的選擇閘示例中的 'a' 或 'b' 一樣使用）。

### CSVarLengthEncodable

實現了 CircuitVarLengthEncodable - 這允許將結構編碼為變量向量（將其想像為序列化為字節）。

### 總結

現在有了上述工具，我們可以使用更複雜的結構在我們的約束系統上進行操作。因此，我們有邏輯閘作為「複雜運算子」，結構作為複雜對象。現在我們準備好提升到下一個層級：電路。

## 電路(Circuits)

電路的定義分佈在 2 個獨立的存儲庫中：`zkevm_circuits` 和 `zkevm_test_harness`。

雖然我們有大約 9 個不同的電路（log_sorter、ram_permutation 等），但在本文中我們將僅聚焦於其中之一：MainVM - 該電路負責處理幾乎所有虛擬機的操作（其他電路用於處理一些預編譯，以及虛擬機執行後發生的操作 - 如準備 pubdata 等）。

查看 zkevm_test_harness，我們可以看到定義：

```rust
pub type VMMainCircuit<F, W, R> =
    ZkSyncUniformCircuitInstance<F, VmMainInstanceSynthesisFunction<F, W, R>>;
```

### 所以什麼是電路?

```rust
pub struct ZkSyncUniformCircuitInstance<F: SmallField, S: ZkSyncUniformSynthesisFunction<F>> {
    // Assignment of values to all the Variables.
    pub witness: AtomicCell<Option<S::Witness>>,

    // Configuration - that is circuit specific, in case of MainVM - the configuration
    // is simply the amount of opcodes that we put within 1 circuit.
    pub config: std::sync::Arc<S::Config>,

    // Circuit 'friendly' hash function.
    pub round_function: std::sync::Arc<S::RoundFunction>,

    // Inputs to the circuits.
    pub expected_public_input: Option<[F; INPUT_OUTPUT_COMMITMENT_LENGTH]>,
}
```

注意 - 該電路沒有任何「顯式」輸出。

其中 ZkSyncUniformCircuitInstance 是一個proxy，因此讓我們更深入地看看主函數：


```rust
impl VmMainInstanceSynthesisFunction {
    fn synthesize_into_cs_inner<CS: ConstraintSystem<F>>(
        cs: &mut CS,
        witness: Self::Witness,
        round_function: &Self::RoundFunction,
        config: Self::Config,
    ) -> [Num<F>; INPUT_OUTPUT_COMMITMENT_LENGTH] {
        main_vm_entry_point(cs, witness, round_function, config)
    }
}

```

這是主要的邏輯，它接受見證（記住，見證是對變量的具體賦值），並返回公共輸入。

如果我們更深入地查看 'main_vm_entry_point'（它已經在 zkevm_circuits 存儲庫中），我們可以看到：


```rust
pub fn main_vm_entry_point(
    cs: &mut CS,
    witness: VmCircuitWitness<F, W>,
    round_function: &R,
    limit: usize,
) -> [Num<F>; INPUT_OUTPUT_COMMITMENT_LENGTH]
```

在這個函數中，我們執行以下操作：

```rust
    // Prepare current 'state'
    //
    // First - unpack the witness
    let VmCircuitWitness {
        closed_form_input,
        witness_oracle,
    } = witness;

    // And add it to the constraint system
    let mut structured_input =
        VmCircuitInputOutput::alloc_ignoring_outputs(cs, closed_form_input.clone());

    let mut state =
        VmLocalState::conditionally_select(cs, start_flag, &bootloader_state, &hidden_fsm_input);

    // Notice, that now state is a VmLocalState object -- which contains 'Variables' inside.

    // And now run the cycles
    for _cycle_idx in 0..limit {
        state = vm_cycle(
            cs,
            state,
            &synchronized_oracle,
            &per_block_context,
            round_function,
        );
    }
```

`vm_cycle` 方法是關鍵所在。它接受給定的操作碼，並在約束系統內創建所有必要的邏輯閘，臨時變數等。這個方法大約有 800 行，所以有興趣的人，我建議你可以看一下。

現在，我們已經為 'limit' 數量的操作碼添加了所有約束，我們必須進行一些額外的管理工作 - 像是存儲隊列雜湊值（用於記憶體、程式碼撤銷等）。

然後，我們準備好處理此方法的結果（input_commitment）。

```rust
    // Prepare compact form (that contains just the hashes of values, rather than full values).
    let compact_form =
        ClosedFormInputCompactForm::from_full_form(cs, &structured_input, round_function);

    // And serialize it.
    let input_commitment: [_; INPUT_OUTPUT_COMMITMENT_LENGTH] =
        commit_variable_length_encodable_item(cs, &compact_form, round_function);
    input_commitment
```

## 現在把所有東西放在一起

現在讓我們看看 zkevm_test_harness 存儲庫，'/src/external_calls.rs' 中的 run 方法。這在許多測試中使用，並嘗試從頭到尾執行整個流程。

儘管這些程式看起來很可怕，我們還是走馬看花一下

```rust
pub fn run<
    F: SmallField,
    R: BuildableCircuitRoundFunction<F, 8, 12, 4> + AlgebraicRoundFunction<F, 8, 12, 4> + serde::Serialize + serde::de::DeserializeOwned,
    H: RecursiveTreeHasher<F, Num<F>>,
    EXT: FieldExtension<2, BaseField = F>,
    S: Storage
>(
caller: Address, // for real block must be zero
entry_point_address: Address, // for real block must be the bootloader
entry_point_code: Vec<[u8; 32]>, // for read lobkc must be a bootloader code
initial_heap_content: Vec<u8>, // bootloader starts with non-deterministic heap
    zk_porter_is_available: bool,
    default_aa_code_hash: U256,
used_bytecodes: std::collections::HashMap<U256, Vec<[u8; 32]>>, // auxiliary information to avoid passing a full set of all used codes
ram_verification_queries: Vec<(u32, U256)>, // we may need to check that after the bootloader's memory is filled
    cycle_limit: usize,
round_function: R, // used for all queues implementation
    geometry: GeometryConfig,
    storage: S,
    tree: &mut impl BinarySparseStorageTree<256, 32, 32, 8, 32, Blake2s256, ZkSyncStorageLeaf>,
) -> (
    BlockBasicCircuits<F, R>,
    BlockBasicCircuitsPublicInputs<F>,
    BlockBasicCircuitsPublicCompactFormsWitnesses<F>,
    SchedulerCircuitInstanceWitness<F, H, EXT>,
    BlockAuxilaryOutputWitness<F>,
)
    where [(); <crate::zkevm_circuits::base_structures::log_query::LogQuery<F> as CSAllocatableExt<F>>::INTERNAL_STRUCT_LEN]:,
    [(); <crate::zkevm_circuits::base_structures::memory_query::MemoryQuery<F> as CSAllocatableExt<F>>::INTERNAL_STRUCT_LEN]:,
    [(); <crate::zkevm_circuits::base_structures::decommit_query::DecommitQuery<F> as CSAllocatableExt<F>>::INTERNAL_STRUCT_LEN]:,
    [(); <crate::boojum::gadgets::u256::UInt256<F> as CSAllocatableExt<F>>::INTERNAL_STRUCT_LEN]:,
    [(); <crate::boojum::gadgets::u256::UInt256<F> as CSAllocatableExt<F>>::INTERNAL_STRUCT_LEN + 1]:,
    [(); <crate::zkevm_circuits::base_structures::vm_state::saved_context::ExecutionContextRecord<F> as CSAllocatableExt<F>>::INTERNAL_STRUCT_LEN]:,
    [(); <crate::zkevm_circuits::storage_validity_by_grand_product::TimestampedStorageLogRecord<F> as CSAllocatableExt<F>>::INTERNAL_STRUCT_LEN]:,

```

第一部分是添加一些解除承諾(decommitments)（稍後解釋）。

然後我們建立一個虛擬機：

```rust
    let mut out_of_circuit_vm =
        create_out_of_circuit_vm(&mut tools, &block_properties, caller, entry_point_address);
```

然後我們對所有操作數進行操作：

```rust
    for _cycle in 0..cycle_limit {

        out_of_circuit_vm
            .cycle(&mut tracer)
            .expect("cycle should finish successfully");
    }
```

在執行過程中，我們會收集「快照」(每個操作數之間系統狀態的詳細信息)。

然後我們創建一個 `Vec<VmInstanceWitness>` - 讓我們看看裡面有什麼：

```rust
pub struct VmInstanceWitness<F: SmallField, O: WitnessOracle<F>> {
    // we need everything to start a circuit from this point of time

    // initial state (state of registers etc)
    pub initial_state: VmLocalState,
    pub witness_oracle: O,
    pub auxilary_initial_parameters: VmInCircuitAuxilaryParameters<F>,
    pub cycles_range: std::ops::Range<u32>,

    // final state for test purposes
    pub final_state: VmLocalState,
    pub auxilary_final_parameters: VmInCircuitAuxilaryParameters<F>,
}
```

有了這個，讓我們最終開始創建電路（藉由 `create_leaf_level_circuits_and_scheduler_witness`）


```rust

    for (instance_idx, vm_instance) in vm_instances_witness.into_iter().enumerate() {
         let instance = VMMainCircuit {
            witness: AtomicCell::new(Some(circuit_input)),
            config: Arc::new(geometry.cycles_per_vm_snapshot as usize),
            round_function: round_function.clone(),
            expected_public_input: Some(proof_system_input),
        };

        main_vm_circuits.push(instance);
    }
```
