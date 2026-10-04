FROM python:3.11-slim

WORKDIR /app
# Copy the local python SDK
COPY sdk/python/forge_sdk /app/sdk/python/forge_sdk
# Copy the worker application
COPY examples/ecommerce-worker /app/examples/ecommerce-worker

# Install requests for the SDK
RUN pip install requests

# Add the SDK to the Python path
ENV PYTHONPATH=/app/sdk/python

WORKDIR /app/examples/ecommerce-worker

CMD ["python", "-u", "main.py"]
