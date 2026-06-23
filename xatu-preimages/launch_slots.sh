#!/bin/bash
# Launch the I/O-capped Xatu slots-distinct as a root systemd transient service.
SVC=xatu-slots
systemctl reset-failed "$SVC" 2>/dev/null || true
systemd-run --collect --unit="$SVC" \
  -p User=0xalizk -p WorkingDirectory=/home/0xalizk/sharded-pir/binary-node \
  -p "IOReadBandwidthMax=/dev/sda 50M" -p "IOWriteBandwidthMax=/dev/sda 50M" -p IOAccounting=yes \
  -p CPUQuota=120% -p Nice=19 \
  bash -c './duckdb < xatu-preimages/distinct_slots.sql > xatu-preimages/distinct/slots.log 2>&1'
echo "launched $SVC"
echo "io.max: $(cat /sys/fs/cgroup/system.slice/$SVC.service/io.max 2>/dev/null)"
