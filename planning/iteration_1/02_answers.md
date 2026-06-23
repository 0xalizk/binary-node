
1. The real object: have a functioning binary-node that is in sync with mainnet block by block. All RPC calls against it should be successful. Measuring Performance is not a primary objective, although basic metric like state size, read/write speeds should collected by that equiv-daemon (any other useful data that can be collected in the execution of that daemon is a good bonus, but the priority remains to being in sync with the tip of the chain and on validating the equivelance between binary and mpt values).



2. "Done" looks like: (1) binary-node is sync'd to the tip of the chain and continues feeding on new blocks as they come in (2) the equiv-daemon has shadowed binary-node since the first block and verified the equivelance of of values with the mainnet-node (3) the dashboard is up and i see the state of mainnet-node, the state of binary-node, and the reporting of equiv-daemon. 




3. "Where does binary-node start its state?" i don't agree these are the only two options. The third option is snap quick sync but the blocks are applied on the binary trie, no archival aspect at all. So Option B (genesis re-exec) shouldn't be significantly slower than a regular full-node sync with mainnet.

4. No i do not care, devp2p-with-one-static-peer (mainnet-node) is what i want. 

5. Yes your "Lean" approach is good. Notice that it is meaningless to check "eth_getProof" because obviously the mainnet-node and binary-node will disagree, totally different tries so the merkle proofs they retun will and should be differet. 


6. equiv-check each block *immediately at head* before advancing

7. continue but record (find *all* divergences and their patterns). If discrepencies exceed 1000, halt, investigate, fix, document, and restart.

8. Yes, reuse the existing Grafana/Prometheus stack

9. keep the tuples (small local store / logs) and surface counts + latest offenders on the dashboard.

10. Yes this box is the only host

11.  Yes **binary-node = custom build from ethrex source** (Rust) on whatever branch has the EIP-7864 work, take what you need from any binary-trie related work the ethrex team has done, you could even look at eip7864 work by geth (a different EL client) and see if you can learn something of value in their approach. I do not care about the code geneology of this binary-node, I just want it to work.

12. run indefinitely, this node will later be used as the data source for other applications via rpc. that will be our next big task. 

13. Go with eip-7864-plan, and treat this as our own fork. 

14. Go with Custom feeder.


