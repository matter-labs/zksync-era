// Populate the sidebar
//
// This is a script, and not included directly in the page, to control the total size of the book.
// The TOC contains an entry for each page, so if each page includes a copy of the TOC,
// the total size of the page becomes O(n**2).
class MDBookSidebarScrollbox extends HTMLElement {
    constructor() {
        super();
    }
    connectedCallback() {
        this.innerHTML = '<ol class="chapter"><li class="chapter-item expanded affix "><a href="index.html">Introduction</a></li><li class="chapter-item expanded affix "><li class="part-title">Guides</li><li class="chapter-item expanded "><a href="guides/index.html"><strong aria-hidden="true">1.</strong> Basic</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="guides/setup-dev.html"><strong aria-hidden="true">1.1.</strong> Setup Dev</a></li><li class="chapter-item expanded "><a href="guides/development.html"><strong aria-hidden="true">1.2.</strong> Development</a></li><li class="chapter-item expanded "><a href="guides/launch.html"><strong aria-hidden="true">1.3.</strong> Launch</a></li><li class="chapter-item expanded "><a href="guides/architecture.html"><strong aria-hidden="true">1.4.</strong> Architecture</a></li><li class="chapter-item expanded "><a href="guides/build-docker.html"><strong aria-hidden="true">1.5.</strong> Build Docker</a></li><li class="chapter-item expanded "><a href="guides/repositories.html"><strong aria-hidden="true">1.6.</strong> Repositories</a></li></ol></li><li class="chapter-item expanded "><a href="guides/advanced/index.html"><strong aria-hidden="true">2.</strong> Advanced</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="guides/advanced/01_initialization.html"><strong aria-hidden="true">2.1.</strong> Local initialization</a></li><li class="chapter-item expanded "><a href="guides/advanced/02_deposits.html"><strong aria-hidden="true">2.2.</strong> Deposits</a></li><li class="chapter-item expanded "><a href="guides/advanced/03_withdrawals.html"><strong aria-hidden="true">2.3.</strong> Withdrawals</a></li><li class="chapter-item expanded "><a href="guides/advanced/04_contracts.html"><strong aria-hidden="true">2.4.</strong> Contracts</a></li><li class="chapter-item expanded "><a href="guides/advanced/05_how_call_works.html"><strong aria-hidden="true">2.5.</strong> Calls</a></li><li class="chapter-item expanded "><a href="guides/advanced/06_how_transaction_works.html"><strong aria-hidden="true">2.6.</strong> Transactions</a></li><li class="chapter-item expanded "><a href="guides/advanced/07_fee_model.html"><strong aria-hidden="true">2.7.</strong> Fee Model</a></li><li class="chapter-item expanded "><a href="guides/advanced/08_how_l2_messaging_works.html"><strong aria-hidden="true">2.8.</strong> L2 Messaging</a></li><li class="chapter-item expanded "><a href="guides/advanced/09_pubdata.html"><strong aria-hidden="true">2.9.</strong> Pubdata</a></li><li class="chapter-item expanded "><a href="guides/advanced/10_pubdata_with_blobs.html"><strong aria-hidden="true">2.10.</strong> Pubdata with Blobs</a></li><li class="chapter-item expanded "><a href="guides/advanced/11_compression.html"><strong aria-hidden="true">2.11.</strong> Bytecode compression</a></li><li class="chapter-item expanded "><a href="guides/advanced/12_alternative_vm_intro.html"><strong aria-hidden="true">2.12.</strong> EraVM intro</a></li><li class="chapter-item expanded "><a href="guides/advanced/13_zk_intuition.html"><strong aria-hidden="true">2.13.</strong> ZK Intuition</a></li><li class="chapter-item expanded "><a href="guides/advanced/14_zk_deeper_overview.html"><strong aria-hidden="true">2.14.</strong> ZK Deeper Dive</a></li><li class="chapter-item expanded "><a href="guides/advanced/15_prover_keys.html"><strong aria-hidden="true">2.15.</strong> Prover Keys</a></li><li class="chapter-item expanded "><a href="guides/advanced/90_advanced_debugging.html"><strong aria-hidden="true">2.16.</strong> Advanced Debugging</a></li><li class="chapter-item expanded "><a href="guides/advanced/91_docker_and_ci.html"><strong aria-hidden="true">2.17.</strong> Docker and CI</a></li></ol></li><li class="chapter-item expanded "><li class="part-title">External Node</li><li class="chapter-item expanded "><a href="guides/external-node/01_intro.html"><strong aria-hidden="true">3.</strong> External node</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="guides/external-node/00_quick_start.html"><strong aria-hidden="true">3.1.</strong> Quick Start</a></li><li class="chapter-item expanded "><a href="guides/external-node/02_configuration.html"><strong aria-hidden="true">3.2.</strong> Configuration</a></li><li class="chapter-item expanded "><a href="guides/external-node/03_running.html"><strong aria-hidden="true">3.3.</strong> Running</a></li><li class="chapter-item expanded "><a href="guides/external-node/04_observability.html"><strong aria-hidden="true">3.4.</strong> Observability</a></li><li class="chapter-item expanded "><a href="guides/external-node/05_troubleshooting.html"><strong aria-hidden="true">3.5.</strong> Troubleshooting</a></li><li class="chapter-item expanded "><a href="guides/external-node/06_components.html"><strong aria-hidden="true">3.6.</strong> Components</a></li><li class="chapter-item expanded "><a href="guides/external-node/07_snapshots_recovery.html"><strong aria-hidden="true">3.7.</strong> Snapshots Recovery</a></li><li class="chapter-item expanded "><a href="guides/external-node/08_pruning.html"><strong aria-hidden="true">3.8.</strong> Pruning</a></li><li class="chapter-item expanded "><a href="guides/external-node/09_treeless_mode.html"><strong aria-hidden="true">3.9.</strong> Treeless Mode</a></li><li class="chapter-item expanded "><a href="guides/external-node/10_decentralization.html"><strong aria-hidden="true">3.10.</strong> Decentralization</a></li></ol></li><li class="chapter-item expanded "><li class="part-title">Specs</li><li class="chapter-item expanded "><a href="specs/introduction.html"><strong aria-hidden="true">4.</strong> Introduction</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="specs/overview.html"><strong aria-hidden="true">4.1.</strong> Overview</a></li><li class="chapter-item expanded "><a href="specs/blocks_batches.html"><strong aria-hidden="true">4.2.</strong> Blocks and Batches</a></li><li class="chapter-item expanded "><a href="specs/l1_smart_contracts.html"><strong aria-hidden="true">4.3.</strong> L1 Smart Contracts</a></li></ol></li><li class="chapter-item expanded "><a href="specs/data_availability/overview.html"><strong aria-hidden="true">5.</strong> Data Availability</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="specs/data_availability/pubdata.html"><strong aria-hidden="true">5.1.</strong> Pubdata</a></li><li class="chapter-item expanded "><a href="specs/data_availability/compression.html"><strong aria-hidden="true">5.2.</strong> Compression</a></li><li class="chapter-item expanded "><a href="specs/data_availability/reconstruction.html"><strong aria-hidden="true">5.3.</strong> Reconstruction</a></li><li class="chapter-item expanded "><a href="specs/data_availability/validium_zk_porter.html"><strong aria-hidden="true">5.4.</strong> Validium ZK Porter</a></li></ol></li><li class="chapter-item expanded "><a href="specs/l1_l2_communication/overview_deposits_withdrawals.html"><strong aria-hidden="true">6.</strong> L1 L2 Communication</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="specs/l1_l2_communication/l1_to_l2.html"><strong aria-hidden="true">6.1.</strong> L1 to L2</a></li><li class="chapter-item expanded "><a href="specs/l1_l2_communication/l2_to_l1.html"><strong aria-hidden="true">6.2.</strong> L2 to L1</a></li></ol></li><li class="chapter-item expanded "><a href="specs/prover/overview.html"><strong aria-hidden="true">7.</strong> Prover</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="specs/prover/getting_started.html"><strong aria-hidden="true">7.1.</strong> Getting Started</a></li><li class="chapter-item expanded "><a href="specs/prover/zk_terminology.html"><strong aria-hidden="true">7.2.</strong> ZK Terminology</a></li><li class="chapter-item expanded "><a href="specs/prover/boojum_function_check_if_satisfied.html"><strong aria-hidden="true">7.3.</strong> Function Check if Satisfied</a></li><li class="chapter-item expanded "><a href="specs/prover/boojum_gadgets.html"><strong aria-hidden="true">7.4.</strong> Gadgets</a></li><li class="chapter-item expanded "><a href="specs/prover/circuit_testing.html"><strong aria-hidden="true">7.5.</strong> Circuit Testing</a></li><li class="chapter-item expanded "><a href="specs/prover/circuits/overview.html"><strong aria-hidden="true">7.6.</strong> Circuits Overview</a></li></ol></li><li class="chapter-item expanded "><a href="specs/zk_chains/overview.html"><strong aria-hidden="true">8.</strong> ZK Chains</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="specs/zk_chains/gateway.html"><strong aria-hidden="true">8.1.</strong> Gateway</a></li><li class="chapter-item expanded "><a href="specs/zk_chains/interop.html"><strong aria-hidden="true">8.2.</strong> Interop</a></li><li class="chapter-item expanded "><a href="specs/zk_chains/shared_bridge.html"><strong aria-hidden="true">8.3.</strong> Shared Bridge</a></li></ol></li><li class="chapter-item expanded "><a href="specs/zk_evm/vm_overview.html"><strong aria-hidden="true">9.</strong> ZK EVM</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="specs/zk_evm/account_abstraction.html"><strong aria-hidden="true">9.1.</strong> Account Abstraction</a></li><li class="chapter-item expanded "><a href="specs/zk_evm/bootloader.html"><strong aria-hidden="true">9.2.</strong> Bootloader</a></li><li class="chapter-item expanded "><a href="specs/zk_evm/fee_model.html"><strong aria-hidden="true">9.3.</strong> Fee Model</a></li><li class="chapter-item expanded "><a href="specs/zk_evm/precompiles.html"><strong aria-hidden="true">9.4.</strong> Precompiles</a></li><li class="chapter-item expanded "><a href="specs/zk_evm/system_contracts.html"><strong aria-hidden="true">9.5.</strong> System Contracts</a></li></ol></li><li class="chapter-item expanded "><li class="part-title">Announcements</li><li class="chapter-item expanded "><a href="announcements/index.html"><strong aria-hidden="true">10.</strong> Announcements</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="announcements/attester_commitee.html"><strong aria-hidden="true">10.1.</strong> Attester Committee</a></li></ol></li></ol>';
        // Set the current, active page, and reveal it if it's hidden
        let current_page = document.location.href.toString();
        if (current_page.endsWith("/")) {
            current_page += "index.html";
        }
        var links = Array.prototype.slice.call(this.querySelectorAll("a"));
        var l = links.length;
        for (var i = 0; i < l; ++i) {
            var link = links[i];
            var href = link.getAttribute("href");
            if (href && !href.startsWith("#") && !/^(?:[a-z+]+:)?\/\//.test(href)) {
                link.href = path_to_root + href;
            }
            // The "index" page is supposed to alias the first chapter in the book.
            if (link.href === current_page || (i === 0 && path_to_root === "" && current_page.endsWith("/index.html"))) {
                link.classList.add("active");
                var parent = link.parentElement;
                if (parent && parent.classList.contains("chapter-item")) {
                    parent.classList.add("expanded");
                }
                while (parent) {
                    if (parent.tagName === "LI" && parent.previousElementSibling) {
                        if (parent.previousElementSibling.classList.contains("chapter-item")) {
                            parent.previousElementSibling.classList.add("expanded");
                        }
                    }
                    parent = parent.parentElement;
                }
            }
        }
        // Track and set sidebar scroll position
        this.addEventListener('click', function(e) {
            if (e.target.tagName === 'A') {
                sessionStorage.setItem('sidebar-scroll', this.scrollTop);
            }
        }, { passive: true });
        var sidebarScrollTop = sessionStorage.getItem('sidebar-scroll');
        sessionStorage.removeItem('sidebar-scroll');
        if (sidebarScrollTop) {
            // preserve sidebar scroll position when navigating via links within sidebar
            this.scrollTop = sidebarScrollTop;
        } else {
            // scroll sidebar to current active section when navigating via "next/previous chapter" buttons
            var activeSection = document.querySelector('#sidebar .active');
            if (activeSection) {
                activeSection.scrollIntoView({ block: 'center' });
            }
        }
        // Toggle buttons
        var sidebarAnchorToggles = document.querySelectorAll('#sidebar a.toggle');
        function toggleSection(ev) {
            ev.currentTarget.parentElement.classList.toggle('expanded');
        }
        Array.from(sidebarAnchorToggles).forEach(function (el) {
            el.addEventListener('click', toggleSection);
        });
    }
}
window.customElements.define("mdbook-sidebar-scrollbox", MDBookSidebarScrollbox);
