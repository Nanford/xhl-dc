import argparse
import asyncio
import getpass
import sys
import traceback

from asyncua import Client


def enum_name(value):
    return getattr(value, "name", str(value))


def print_endpoint(endpoint, index):
    print(f"[{index}] endpoint_url: {endpoint.EndpointUrl}")
    print(f"    security_policy_uri: {endpoint.SecurityPolicyUri}")
    print(f"    security_mode: {enum_name(endpoint.SecurityMode)}")
    print(f"    security_level: {endpoint.SecurityLevel}")

    tokens = endpoint.UserIdentityTokens or []
    if not tokens:
        print("    user_tokens: <none>")
        return

    print("    user_tokens:")
    for token in tokens:
        print(f"      - policy_id: {token.PolicyId}")
        print(f"        token_type: {enum_name(token.TokenType)}")
        policy = token.SecurityPolicyUri or "<empty>"
        print(f"        security_policy_uri: {policy}")


async def discover(endpoint, timeout):
    client = Client(endpoint, timeout=timeout)
    endpoints = await client.connect_and_get_server_endpoints()
    print(f"discovered_endpoints: {len(endpoints)}")
    for index, endpoint_description in enumerate(endpoints, start=1):
        print_endpoint(endpoint_description, index)
    return endpoints


async def try_connect(args, username=None, password=None):
    label = "username" if username else "anonymous"
    print(f"\nconnect_test: {label}")

    client = Client(args.endpoint, timeout=args.timeout)
    if username:
        client.set_user(username)
        client.set_password(password or "")

    connected = False
    try:
        await client.connect()
        connected = True
        node = client.get_node(args.node)
        value = await node.read_value()
        print(f"result: ok")
        print(f"read_node: {args.node}")
        print(f"read_value: {value!r}")
        return True
    except Exception as exc:
        print("result: failed")
        print(f"error_type: {type(exc).__name__}")
        print(f"error: {exc!r}")
        if args.verbose:
            traceback.print_exc()
        return False
    finally:
        if connected:
            await client.disconnect()


async def main():
    parser = argparse.ArgumentParser(
        description="Probe a Kepware OPC UA endpoint with asyncua."
    )
    parser.add_argument("endpoint", help="OPC UA endpoint, for example opc.tcp://127.0.0.1:49320")
    parser.add_argument("--timeout", type=float, default=5.0, help="socket timeout in seconds")
    parser.add_argument("--node", default="i=2258", help="node to read after connect; default is ServerStatus.CurrentTime")
    parser.add_argument("--username", help="optional OPC UA username")
    parser.add_argument("--password", help="optional OPC UA password; if omitted with --username, prompt securely")
    parser.add_argument("--skip-anonymous", action="store_true", help="do not attempt anonymous connect")
    parser.add_argument("--verbose", action="store_true", help="print full Python traceback on errors")
    args = parser.parse_args()

    print(f"endpoint: {args.endpoint}")
    try:
        await discover(args.endpoint, args.timeout)
    except Exception as exc:
        print("discovery_result: failed")
        print(f"error_type: {type(exc).__name__}")
        print(f"error: {exc!r}")
        if args.verbose:
            traceback.print_exc()
        return 2

    ok = True
    if not args.skip_anonymous:
        ok = await try_connect(args)

    if args.username:
        password = args.password
        if password is None:
            password = getpass.getpass("OPC UA password: ")
        user_ok = await try_connect(args, username=args.username, password=password)
        ok = ok and user_ok if not args.skip_anonymous else user_ok

    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
