import argparse
import asyncio
import getpass
import pathlib
import re
import sys
from dataclasses import dataclass

import yaml
from asyncua import Client, ua


SUPPORTED_DATA_TYPES = {
    "Boolean",
    "SByte",
    "Byte",
    "Int16",
    "UInt16",
    "Int32",
    "UInt32",
    "Int64",
    "UInt64",
    "Float",
    "Double",
    "String",
    "ByteString",
}

SYSTEM_PATH_MARKERS = {"_System", "_Statistics"}
DEFAULT_TARGET_SUBSCRIPTION = "fast"
DEFAULT_MIN_NAMESPACE_INDEX = 2
DEFAULT_MAX_DEPTH_COUNT = 12
DEFAULT_MAX_TAGS_COUNT = 500


@dataclass(frozen=True)
class TagCandidate:
    node_id: str
    browse_path: list[str]
    data_type: str
    value_rank: int | None


@dataclass(frozen=True)
class DiscoveryOptions:
    enabled: bool
    target_subscription: str
    root_node_ids: list[str]
    include_paths: list[list[str]]
    exclude_paths: list[list[str]]
    min_namespace_index: int
    max_depth_count: int
    max_tags_count: int
    include_system: bool
    include_arrays: bool


def namespace_index_from_node_id(node_id: str) -> int:
    match = re.match(r"^ns=(\d+);", node_id)
    if match:
        return int(match.group(1))
    return 0


def is_system_path(path: list[str]) -> bool:
    return any(part in SYSTEM_PATH_MARKERS or part.startswith("_") for part in path)


def parse_include_path(value: str) -> list[str]:
    return [part.strip() for part in value.split(".") if part.strip()]


def matches_include_paths(path: list[str], include_paths: list[list[str]]) -> bool:
    if not include_paths:
        return True
    return any(path[: len(include_path)] == include_path for include_path in include_paths)


def matches_exclude_paths(path: list[str], exclude_paths: list[list[str]]) -> bool:
    return any(path[: len(exclude_path)] == exclude_path for exclude_path in exclude_paths)


def should_collect_candidate(
    candidate: TagCandidate,
    min_namespace: int,
    include_system: bool,
    include_arrays: bool,
    include_paths: list[list[str]] | None = None,
    exclude_paths: list[list[str]] | None = None,
) -> bool:
    include_paths = include_paths or []
    exclude_paths = exclude_paths or []
    if namespace_index_from_node_id(candidate.node_id) < min_namespace:
        return False
    if not include_system and is_system_path(candidate.browse_path):
        return False
    if not matches_include_paths(candidate.browse_path, include_paths):
        return False
    if matches_exclude_paths(candidate.browse_path, exclude_paths):
        return False
    if candidate.data_type not in SUPPORTED_DATA_TYPES:
        return False
    if not include_arrays and candidate.value_rank not in (None, -1):
        return False
    return True


def make_alias(candidate: TagCandidate, max_len: int = 64) -> str:
    parts = [part.strip() for part in candidate.browse_path if part.strip()]
    raw = "_".join(parts[-4:]) if parts else candidate.node_id
    alias = re.sub(r"\W+", "_", raw, flags=re.UNICODE).strip("_")
    if not alias:
        alias = re.sub(r"\W+", "_", candidate.node_id, flags=re.UNICODE).strip("_")
    if len(alias) <= max_len:
        return alias
    return alias[-max_len:]


def build_tags(candidates: list[TagCandidate], max_alias_len: int = 64) -> list[dict[str, str]]:
    used: dict[str, int] = {}
    tags = []
    for candidate in sorted(candidates, key=lambda item: item.node_id):
        base_alias = make_alias(candidate, max_alias_len)
        count = used.get(base_alias, 0) + 1
        used[base_alias] = count
        alias = base_alias
        if count > 1:
            suffix = f"_{count}"
            alias = f"{base_alias[: max_alias_len - len(suffix)]}{suffix}"
        tags.append({"node_id": candidate.node_id, "alias": alias})
    return tags


def replace_subscription_tags(config: dict, subscription_name: str, tags: list[dict[str, str]]) -> dict:
    subscriptions = config.get("subscriptions")
    if not isinstance(subscriptions, list):
        raise ValueError("config has no subscriptions list")

    for subscription in subscriptions:
        if subscription.get("name") == subscription_name:
            subscription["tags"] = tags
            return config

    raise ValueError(f"subscription {subscription_name!r} not found")


def load_config(path: pathlib.Path) -> dict:
    with path.open("r", encoding="utf-8-sig") as file:
        return yaml.safe_load(file)


def write_config(path: pathlib.Path, config: dict) -> None:
    content = yaml.safe_dump(config, allow_unicode=True, sort_keys=False, default_flow_style=False)
    path.write_text(content, encoding="utf-8", newline="\n")


def list_of_strings(value, field: str) -> list[str]:
    if value is None:
        return []
    if not isinstance(value, list):
        raise ValueError(f"opcua.discovery.{field} must be a list")

    items = []
    for item in value:
        if not isinstance(item, str) or not item.strip():
            raise ValueError(f"opcua.discovery.{field} must contain non-empty strings")
        items.append(item.strip())
    return items


def positive_int(value, field: str, default: int) -> int:
    if value is None:
        return default
    if not isinstance(value, int) or value <= 0:
        raise ValueError(f"opcua.discovery.{field} must be greater than 0")
    return value


def bool_value(value, field: str, default: bool) -> bool:
    if value is None:
        return default
    if not isinstance(value, bool):
        raise ValueError(f"opcua.discovery.{field} must be true or false")
    return value


def discovery_options_from_config(config: dict) -> DiscoveryOptions:
    opcua = config.get("opcua") if isinstance(config, dict) else {}
    discovery = opcua.get("discovery") if isinstance(opcua, dict) else {}
    enabled = bool_value(discovery.get("enabled"), "enabled", False) if isinstance(discovery, dict) else False
    if not enabled:
        discovery = {}

    target_subscription = discovery.get("target_subscription", DEFAULT_TARGET_SUBSCRIPTION)
    if not isinstance(target_subscription, str) or not target_subscription.strip():
        raise ValueError("opcua.discovery.target_subscription must not be empty")

    include_paths = [parse_include_path(value) for value in list_of_strings(discovery.get("include_paths"), "include_paths")]
    exclude_paths = [parse_include_path(value) for value in list_of_strings(discovery.get("exclude_paths"), "exclude_paths")]

    return DiscoveryOptions(
        enabled=enabled,
        target_subscription=target_subscription.strip(),
        root_node_ids=list_of_strings(discovery.get("root_node_ids"), "root_node_ids"),
        include_paths=include_paths,
        exclude_paths=exclude_paths,
        min_namespace_index=positive_int(
            discovery.get("min_namespace_index"),
            "min_namespace_index",
            DEFAULT_MIN_NAMESPACE_INDEX,
        ),
        max_depth_count=positive_int(
            discovery.get("max_depth_count"),
            "max_depth_count",
            DEFAULT_MAX_DEPTH_COUNT,
        ),
        max_tags_count=positive_int(
            discovery.get("max_tags_count"),
            "max_tags_count",
            DEFAULT_MAX_TAGS_COUNT,
        ),
        include_system=bool_value(discovery.get("include_system"), "include_system", False),
        include_arrays=bool_value(discovery.get("include_arrays"), "include_arrays", False),
    )


def node_id_to_string(nodeid) -> str:
    to_string = getattr(nodeid, "to_string", None)
    if callable(to_string):
        return to_string()
    return str(nodeid)


async def read_data_type_name(client: Client, node) -> str:
    data_type_nodeid = await node.read_data_type()
    data_type_node = client.get_node(data_type_nodeid)
    browse_name = await data_type_node.read_browse_name()
    return browse_name.Name


async def read_value_rank(node) -> int | None:
    try:
        return await node.read_value_rank()
    except Exception:
        return None


async def browse_candidates(
    client: Client,
    min_namespace: int,
    max_depth: int,
    include_system: bool,
    include_arrays: bool,
    include_paths: list[list[str]] | None = None,
    exclude_paths: list[list[str]] | None = None,
    root_node_ids: list[str] | None = None,
) -> list[TagCandidate]:
    include_paths = include_paths or []
    exclude_paths = exclude_paths or []
    root_node_ids = root_node_ids or []
    candidates: list[TagCandidate] = []
    visited: set[str] = set()

    async def browse_node(node, path: list[str], depth: int) -> None:
        if depth > max_depth:
            return

        try:
            children = await node.get_children()
        except Exception as exc:
            print(f"warn: failed to browse {'/'.join(path) or '<objects>'}: {exc}", file=sys.stderr)
            return

        for child in children:
            child_node_id = node_id_to_string(child.nodeid)
            if child_node_id in visited:
                continue
            visited.add(child_node_id)

            try:
                browse_name = await child.read_browse_name()
                child_path = path + [browse_name.Name]
                node_class = await child.read_node_class()
            except Exception as exc:
                print(f"warn: failed to read node metadata {child_node_id}: {exc}", file=sys.stderr)
                continue

            namespace_index = getattr(child.nodeid, "NamespaceIndex", namespace_index_from_node_id(child_node_id))

            if node_class == ua.NodeClass.Variable:
                try:
                    candidate = TagCandidate(
                        node_id=child_node_id,
                        browse_path=child_path,
                        data_type=await read_data_type_name(client, child),
                        value_rank=await read_value_rank(child),
                    )
                except Exception as exc:
                    print(f"warn: failed to read variable metadata {child_node_id}: {exc}", file=sys.stderr)
                    continue

                if should_collect_candidate(
                    candidate,
                    min_namespace,
                    include_system,
                    include_arrays,
                    include_paths,
                    exclude_paths,
                ):
                    candidates.append(candidate)
                continue

            if node_class in (ua.NodeClass.Object, ua.NodeClass.View) and namespace_index >= min_namespace:
                if include_system or not is_system_path(child_path):
                    await browse_node(child, child_path, depth + 1)

    if root_node_ids:
        for root_node_id in root_node_ids:
            root_node = client.get_node(root_node_id)
            try:
                root_browse_name = await root_node.read_browse_name()
                root_path = [root_browse_name.Name]
            except Exception as exc:
                print(f"warn: failed to read root node metadata {root_node_id}: {exc}", file=sys.stderr)
                root_path = []
            await browse_node(root_node, root_path, 0)
    else:
        await browse_node(client.get_objects_node(), [], 0)
    return candidates


def print_summary(candidates: list[TagCandidate], tags: list[dict[str, str]], limit: int) -> None:
    type_counts: dict[str, int] = {}
    for candidate in candidates:
        type_counts[candidate.data_type] = type_counts.get(candidate.data_type, 0) + 1

    print(f"collected_candidates: {len(candidates)}")
    if limit and len(tags) >= limit and len(candidates) > limit:
        print(f"tags_limited_to: {limit}")
    print("data_type_counts:")
    for data_type, count in sorted(type_counts.items()):
        print(f"  {data_type}: {count}")

    print("sample_tags:")
    for tag in tags[:20]:
        print(f"  - node_id: {tag['node_id']}, alias: {tag['alias']}")


async def run(args) -> int:
    config_path = pathlib.Path(args.config)
    config = load_config(config_path)
    discovery_options = discovery_options_from_config(config)
    endpoint = args.endpoint or config.get("opcua", {}).get("endpoint")
    if not endpoint:
        print("error: endpoint missing; pass --endpoint or set opcua.endpoint in config", file=sys.stderr)
        return 2
    subscription = args.subscription or discovery_options.target_subscription
    max_depth = args.max_depth if args.max_depth is not None else discovery_options.max_depth_count
    min_namespace = (
        args.min_namespace
        if args.min_namespace is not None
        else discovery_options.min_namespace_index
    )
    limit = args.limit if args.limit is not None else discovery_options.max_tags_count
    include_system = args.include_system or discovery_options.include_system
    include_arrays = args.include_arrays or discovery_options.include_arrays
    root_node_ids = args.root_node_id or discovery_options.root_node_ids

    client = Client(endpoint, timeout=args.timeout)
    if args.username:
        password = args.password
        if password is None:
            password = getpass.getpass("OPC UA password: ")
        client.set_user(args.username)
        client.set_password(password)

    print(f"endpoint: {endpoint}")
    include_paths = discovery_options.include_paths + [parse_include_path(value) for value in args.include_path]
    exclude_paths = discovery_options.exclude_paths + [parse_include_path(value) for value in args.exclude_path]
    for include_path in include_paths:
        if not include_path:
            print("error: --include-path must not be empty", file=sys.stderr)
            return 2
    for exclude_path in exclude_paths:
        if not exclude_path:
            print("error: --exclude-path must not be empty", file=sys.stderr)
            return 2
    await client.connect()
    try:
        candidates = await browse_candidates(
            client,
            min_namespace=min_namespace,
            max_depth=max_depth,
            include_system=include_system,
            include_arrays=include_arrays,
            include_paths=include_paths,
            exclude_paths=exclude_paths,
            root_node_ids=root_node_ids,
        )
    finally:
        await client.disconnect()

    tags = build_tags(candidates)
    if limit > 0:
        tags = tags[:limit]

    print_summary(candidates, tags, limit)

    if args.write:
        replace_subscription_tags(config, subscription, tags)
        write_config(config_path, config)
        print(f"updated_config: {config_path}")
        print(f"updated_subscription: {subscription}")
        print(f"written_tags: {len(tags)}")
    else:
        print("dry_run: true")
    return 0


def parse_args():
    parser = argparse.ArgumentParser(description="Browse Kepware OPC UA tags and update config tags.")
    parser.add_argument("--config", default="config.local.yaml", help="YAML config to read/update")
    parser.add_argument("--endpoint", help="OPC UA endpoint; defaults to opcua.endpoint in config")
    parser.add_argument("--subscription", help="subscription name to replace; defaults to opcua.discovery.target_subscription")
    parser.add_argument("--write", action="store_true", help="write discovered tags into the config")
    parser.add_argument("--timeout", type=float, default=8.0, help="OPC UA socket timeout in seconds")
    parser.add_argument("--max-depth", type=int, help="maximum browse recursion depth")
    parser.add_argument("--min-namespace", type=int, help="minimum namespace index to collect")
    parser.add_argument("--limit", type=int, help="maximum tags to write; 0 means no limit")
    parser.add_argument("--include-system", action="store_true", help="include _System and _Statistics branches")
    parser.add_argument("--include-arrays", action="store_true", help="include array-valued tags")
    parser.add_argument(
        "--root-node-id",
        action="append",
        default=[],
        help="browse from this root NodeId instead of Objects; may be repeated",
    )
    parser.add_argument(
        "--include-path",
        action="append",
        default=[],
        help="only collect tags whose browse path starts with this dot-separated branch; may be repeated",
    )
    parser.add_argument(
        "--exclude-path",
        action="append",
        default=[],
        help="skip tags whose browse path starts with this dot-separated branch; may be repeated",
    )
    parser.add_argument("--username", help="optional OPC UA username")
    parser.add_argument("--password", help="optional OPC UA password")
    return parser.parse_args()


if __name__ == "__main__":
    sys.exit(asyncio.run(run(parse_args())))
