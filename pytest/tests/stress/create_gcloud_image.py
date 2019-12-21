import sys
import os
import datetime
from rc import gcloud

try:
    image_name = sys.argv[1]
except:
    image_name = f'near-staging-{datetime.datetime.strftime(datetime.datetime.now(),"%Y%m%d")}-{os.getlogin()}'

machine_name = f'{image_name}-image-builder'

m = gcloud.create(
    name=machine_name,
    machine_type='n1-standard-64',
    disk_size='50G',
    image_project='ubuntu-os-cloud',
    image_family='ubuntu-1804-lts',
    zone='us-west2-c',
    firewall_allows=['tcp:3030', 'tcp:24567'],
    min_cpu_platform='Intel Skylake'
)

print('machine created')

p = m.run('bash', input='''
for i in `seq 1 3`; do
    sudo apt update
done

sudo apt install -y python pkg-config libssl-dev build-essential cmake clang llvm

curl -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain nightly-2019-10-04
source ~/.cargo/env

git clone --single-branch --branch staging https://github.com/nearprotocol/nearcore.git nearcore
cd nearcore
cargo build --workspace --release

''')

assert p.returncode == 0

print('near built')

m.stop()

print('machine stopped')

m.save_image(image_name)

print('image saved')

m.delete()

print('machine deleted')