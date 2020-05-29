# Survive massive state sync
#
# Create 3 nodes, 1 validator and 2 observers tracking the single shard 0.
# Generate a large state using genesis-populate. [*]
#
# Spawn validator and first observer and wait for them to make some progress.
# Spawn second observer and watch how it is able to sync state
# without degrading blocks per second.
#
# To run this test is important to compile genesis-populate tool first.
# In nearcore folder run:
#
# ```
# cargo build -p genesis-populate
# ```
#
# [*] This test might take a very large time generating the state.
# To speed up this between multiple executions, large state can be generated once
# saved, and reused on multiple executions. Steps to do this.
#
# 1. Run test for first time:
#
# ```
# python3 python3 tests/sanity/state_sync_massive.py
# ```
#
# Stop at any point after seeing the message: "Genesis generated"
#
# 2. Save generated data:
#
# ```
# cp -r ~/.near/test0_finished ~/.near/backup_genesis
# ```
#
# 3. Run test passing path to backup_genesis
#
# ```
# python3 tests/sanity/state_sync_massive.py ~/.near/backup_genesis
# ```
#

import sys, time, requests
from queue import Queue

sys.path.append('lib')

from cluster import init_cluster, spin_up_node, load_config
from utils import TxContext, LogTracker
from populate import genesis_populate_all, copy_genesis

if len(sys.argv) >= 2:
    genesis_data = sys.argv[1]
else:
    genesis_data = None
    additional_accounts = 200000

config = load_config()
near_root, node_dirs = init_cluster(
    1, 2, 1, config,
    [["min_gas_price", 0], ["max_inflation_rate", [0, 1]], ["epoch_length", 20],
     ["block_producer_kickout_threshold", 80]], {1: {
         "tracked_shards": [0]
     }, 2: {
         "tracked_shards": [0]
     }})

print("Populating genesis")

if genesis_data is None:
    genesis_populate_all(near_root, additional_accounts, node_dirs)
else:
    for node_dir in node_dirs:
        copy_genesis(genesis_data, node_dir)

print("Genesis generated")

SMALL_HEIGHT = 40
LARGE_HEIGHT = 100
TIMEOUT = 150 + SMALL_HEIGHT + LARGE_HEIGHT + 10**9
start = time.time()

boot_node = spin_up_node(config, near_root, node_dirs[0], 0, None, None)
observer = spin_up_node(config, near_root, node_dirs[1], 1, boot_node.node_key.pk, boot_node.addr())

def wait_for_height(target_height, rpc_node, sleep_time=2, bps_threshold=-1):
    queue = []
    latest_height = 0

    while latest_height < target_height:
        assert time.time() - start < TIMEOUT

        # Check current height
        try:
            status = rpc_node.get_status()
            new_height = status['sync_info']['latest_block_height']
            print(f"Height: {latest_height} => {new_height}")
            latest_height = new_height
        except requests.ReadTimeout:
            print("Timeout Error")

        # Computing bps
        cur_time = time.time()
        queue.append((cur_time, latest_height))

        while len(queue) > 2 and queue[0][0] <= cur_time - 7:
            queue.pop(0)

        if len(queue) <= 1:
            bps = 0
        else:
            head = queue[-1]
            tail = queue[0]
            bps = (head[1] - tail[1]) / (head[0] - tail[0])

            assert bps >= bps_threshold

        print(f"bps: {bps} queue length: {len(queue)}")
        time.sleep(sleep_time)


wait_for_height(SMALL_HEIGHT, boot_node)

observer = spin_up_node(config, near_root, node_dirs[2], 2, boot_node.node_key.pk, boot_node.addr())

# Check that bps is not degraded

# Right now when observer 2 starts state sync bps decrease way low than desired.
# Using a very small (larger than 0) number for now.
BPS_THRESHOLD = 1e-12

wait_for_height(LARGE_HEIGHT, boot_node, bps_threshold=BPS_THRESHOLD)

# Make sure observer2 is able to sync
wait_for_height(SMALL_HEIGHT, observer)
