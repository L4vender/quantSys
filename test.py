import os
from py_clob_client.client import ClobClient

HOST = "https://clob.polymarket.com"
CHAIN_ID = 137

private_key = os.getenv("PRIVATE_KEY")

if not private_key:
    raise RuntimeError("请先设置环境变量 PRIVATE_KEY")

client = ClobClient(
    host=HOST,
    key=private_key,
    chain_id=CHAIN_ID,
)

creds = client.create_or_derive_api_creds()

print("POLYMARKET_API_KEY =", creds.api_key)
print("POLYMARKET_SECRET =", creds.api_secret)
print("POLYMARKET_PASSPHRASE =", creds.api_passphrase)