cd /Users/lhy/Code/quantSys

mkdir -p output/local-csv/_run_logs
if [ -f .env ]; then set -a; . ./.env; set +a; fi

RUN_ID="six_hour_$(date -u +%Y%m%dT%H%M%SZ)"
RUN_DIR="output/local-csv/_run_logs/$RUN_ID"
mkdir -p "$RUN_DIR"

THER_LOG="$RUN_DIR/therundown_ws.log"
POLY_LOG="$RUN_DIR/polymarket_ws.log"

cargo run -p adapter-therundown -- \
  --config configs/sources/therundown.example.toml \
  --mode ws \
  --csv-output output/local-csv \
  > "$THER_LOG" 2>&1 &
THER_PID=$!

HTTPS_PROXY=http://127.0.0.1:6244 \
HTTP_PROXY=http://127.0.0.1:6244 \
RUST_LOG=adapter_polymarket_market=info \
cargo run -p adapter-polymarket-market -- \
  --config configs/sources/polymarket.example.toml \
  --mode market-ws \
  --csv-output output/local-csv \
  > "$POLY_LOG" 2>&1 &
POLY_PID=$!

echo "THER_PID=$THER_PID"
echo "POLY_PID=$POLY_PID"
echo "RUN_DIR=$RUN_DIR"

sleep 21600

kill -TERM "$THER_PID" "$POLY_PID"