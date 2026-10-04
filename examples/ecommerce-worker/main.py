import os
import sys

# Add local SDK to path for demonstration purposes
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), '../../sdk/python')))

from forge_sdk import ForgeWorker, JobContext

# 1. Initialize the worker
worker = ForgeWorker(
    base_url=os.environ.get("FORGE_API_URL", "http://localhost:3000/api/v1"),
    tenant_id=os.environ.get("FORGE_TENANT_ID", "default-tenant"),
    api_key=os.environ.get("FORGE_API_KEY", "dummy-key")
)

# 2. Define business logic using simple decorators

@worker.job("extract-orders")
def handle_extract(ctx: JobContext):
    ctx.log("Extracting new orders from database...")
    # Simulate DB query
    orders = [
        {"order_id": "ORD-001", "amount": 150.00, "user_score": 0.8},
        {"order_id": "ORD-002", "amount": 9999.00, "user_score": 0.1}, # Suspicious
        {"order_id": "ORD-003", "amount": 25.50, "user_score": 0.9}
    ]
    ctx.log(f"Extracted {len(orders)} orders.")
    return {"orders": orders}

@worker.job("fraud-check")
def handle_fraud_check(ctx: JobContext):
    # In a real DAG, the payload from the previous step is passed in
    orders = ctx.payload.get("orders", [])
    ctx.log(f"Running ML fraud detection on {len(orders)} orders...")
    
    flagged = []
    approved = []
    
    for order in orders:
        # Simulate ML model scoring
        risk_score = (order["amount"] / 10000.0) + (1.0 - order["user_score"])
        
        if risk_score > 0.8:
            ctx.log(f"Order {order['order_id']} flagged as HIGH RISK (Score: {risk_score:.2f})")
            flagged.append(order)
        else:
            approved.append(order)
            
    return {
        "approved_orders": approved,
        "flagged_orders": flagged
    }

@worker.job("settle-payment")
def handle_settle(ctx: JobContext):
    approved_orders = ctx.payload.get("approved_orders", [])
    ctx.log(f"Settling payments for {len(approved_orders)} approved orders.")
    
    total_settled = 0
    for order in approved_orders:
        ctx.log(f"Charging credit card for order {order['order_id']} (${order['amount']})...")
        total_settled += order['amount']
        
    return {"status": "SUCCESS", "total_settled": total_settled}

@worker.job("notify-risk-team")
def handle_notify(ctx: JobContext):
    flagged_orders = ctx.payload.get("flagged_orders", [])
    if not flagged_orders:
        ctx.log("No flagged orders to notify.")
        return {"status": "SKIPPED"}
        
    ctx.log(f"Sending Slack alert to Risk team for {len(flagged_orders)} orders!")
    for order in flagged_orders:
        ctx.log(f"Alert: Review Order {order['order_id']}")
        
    return {"status": "NOTIFIED", "count": len(flagged_orders)}


if __name__ == "__main__":
    print("Starting E-Commerce Fraud Worker...")
    # Start polling the 'ecommerce-queue'
    worker.start(queue="ecommerce-queue")
