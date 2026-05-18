import argparse
import pathlib
import re
import sys
import xml.etree.ElementTree as ET
from dataclasses import dataclass

import yaml


DEFAULT_NAMESPACE_INDEX = 2
DEFAULT_MAX_TAGS_PER_SUBSCRIPTION = 1000
DEFAULT_MONITORED_ITEM_CREATE_BATCH_SIZE = 500
DEFAULT_SUBSCRIPTION_PREFIX = "cpk_alarm"
DEFAULT_ROUTE_TABLE = "cpk_alarm_log"
DEFAULT_DESCRIPTION_MAP_PATH = "./description_map.cpk.yaml"


class QuotedStringDumper(yaml.SafeDumper):
    pass


def quoted_string_representer(dumper, value: str):
    return dumper.represent_scalar("tag:yaml.org,2002:str", value, style='"')


QuotedStringDumper.add_representer(str, quoted_string_representer)


@dataclass(frozen=True)
class AlarmPoint:
    tag_name: str
    description: str
    group_name: str = ""
    source_row: str = ""


@dataclass(frozen=True)
class AlarmPointDocument:
    declared_total_count: int | None
    points: list[AlarmPoint]


def load_alarm_point_document(path: pathlib.Path) -> AlarmPointDocument:
    root = ET.parse(path).getroot()
    if root.tag == "OpcUaPointList":
        return load_opcua_point_list(root)
    if root.tag == "OpcUaAlarmPoints":
        return load_opcua_alarm_points(root)
    raise ValueError(f"expected OpcUaPointList or OpcUaAlarmPoints root, got {root.tag!r}")


def load_opcua_point_list(root: ET.Element) -> AlarmPointDocument:
    declared_total_count = parse_optional_int(root.get("totalCount"), "totalCount")
    points: list[AlarmPoint] = []
    for group in root.findall("Group"):
        group_name = (group.get("name") or "").strip()
        for point in group.findall("Point"):
            points.append(
                AlarmPoint(
                    tag_name=child_text(point, "TagName"),
                    description=child_text(point, "Description"),
                    group_name=group_name,
                    source_row=(point.get("sourceRow") or "").strip(),
                )
            )

    return AlarmPointDocument(declared_total_count=declared_total_count, points=points)


def load_opcua_alarm_points(root: ET.Element) -> AlarmPointDocument:
    declared_total_count = parse_optional_int(
        child_text(root.find("Summary"), "TotalCount"), "TotalCount"
    )
    points: list[AlarmPoint] = []
    sources = root.findall("./Sources/Source")
    if not sources:
        # Some generated point-list exports place Source directly below the root.
        sources = root.findall("./Source")
    for source in sources:
        source_name = (source.get("name") or "").strip()
        for point in source.findall("./Points/Point"):
            source_row = (point.get("sourceRow") or "").strip()
            file_name = (source.get("fileName") or "").strip()
            if file_name and source_row:
                source_row = f"{file_name}:{source_row}"
            points.append(
                AlarmPoint(
                    tag_name=child_text(point, "TagName"),
                    description=child_text(point, "Description"),
                    group_name=source_name,
                    source_row=source_row,
                )
            )

    return AlarmPointDocument(declared_total_count=declared_total_count, points=points)


def parse_optional_int(value: str | None, field: str) -> int | None:
    if value is None or not value.strip():
        return None
    try:
        return int(value)
    except ValueError as exc:
        raise ValueError(f"{field} must be an integer") from exc


def child_text(parent: ET.Element, name: str) -> str:
    child = parent.find(name)
    if child is None or child.text is None:
        return ""
    return child.text.strip()


def validate_alarm_points(
    document: AlarmPointDocument,
    allow_empty_descriptions: bool = False,
) -> None:
    if (
        document.declared_total_count is not None
        and document.declared_total_count != len(document.points)
    ):
        raise ValueError(
            f"declared totalCount {document.declared_total_count} does not match {len(document.points)} points"
        )

    for index, point in enumerate(document.points, start=1):
        if not point.tag_name:
            raise ValueError(f"empty TagName at point {index}")
        if not point.description and not allow_empty_descriptions:
            raise ValueError(f"empty Description for {point.tag_name}")

    seen: set[str] = set()
    for point in document.points:
        if point.tag_name in seen:
            raise ValueError(f"duplicate TagName {point.tag_name}")
        seen.add(point.tag_name)


def node_id_for_tag_name(tag_name: str, namespace_index: int = DEFAULT_NAMESPACE_INDEX) -> str:
    return f"ns={namespace_index};s={tag_name}"


def tag_prefix(tag_name: str) -> str:
    prefix = tag_name.split(".", 1)[0].strip()
    if not prefix:
        raise ValueError(f"cannot derive prefix from TagName {tag_name!r}")
    return prefix


def build_description_map(
    points: list[AlarmPoint],
    namespace_index: int = DEFAULT_NAMESPACE_INDEX,
) -> dict[str, str]:
    return {
        node_id_for_tag_name(point.tag_name, namespace_index): point.description
        for point in sorted(points, key=lambda item: item.tag_name)
        if point.description
    }


def build_subscriptions(
    points: list[AlarmPoint],
    max_tags_per_subscription: int = DEFAULT_MAX_TAGS_PER_SUBSCRIPTION,
    namespace_index: int = DEFAULT_NAMESPACE_INDEX,
    subscription_prefix: str = DEFAULT_SUBSCRIPTION_PREFIX,
) -> list[dict]:
    if max_tags_per_subscription <= 0:
        raise ValueError("max_tags_per_subscription must be greater than 0")

    groups: dict[str, list[AlarmPoint]] = {}
    for point in points:
        groups.setdefault(tag_prefix(point.tag_name), []).append(point)

    subscriptions: list[dict] = []
    for prefix in sorted(groups):
        group_points = groups[prefix]
        for index, chunk in enumerate(chunks(group_points, max_tags_per_subscription), start=1):
            subscriptions.append(
                {
                    "name": f"{subscription_prefix}_{safe_name(prefix)}_{index:03d}",
                    "publishing_interval_ms": 500,
                    "keep_alive_count": 10,
                    "lifetime_count": 30,
                    "max_notifications_per_publish": 0,
                    "priority": 0,
                    "tags": [
                        {
                            "node_id": node_id_for_tag_name(point.tag_name, namespace_index),
                            "alias": point.tag_name,
                        }
                        for point in chunk
                    ],
                }
            )
    return subscriptions


def chunks(items: list[AlarmPoint], size: int):
    for start in range(0, len(items), size):
        yield items[start : start + size]


def safe_name(value: str) -> str:
    return re.sub(r"[^0-9A-Za-z]+", "_", value).strip("_").lower()


def update_config(
    config: dict,
    subscriptions: list[dict],
    route_prefixes: set[str],
    description_map_path: str = DEFAULT_DESCRIPTION_MAP_PATH,
    route_table: str = DEFAULT_ROUTE_TABLE,
    subscription_file_path: str | None = None,
) -> dict:
    opcua = config.setdefault("opcua", {})
    opcua["description_map_path"] = description_map_path
    opcua.setdefault(
        "monitored_item_create_batch_size_count",
        DEFAULT_MONITORED_ITEM_CREATE_BATCH_SIZE,
    )

    if subscription_file_path:
        files = opcua.setdefault("subscription_files", [])
        if not isinstance(files, list):
            raise ValueError("opcua.subscription_files must be a list")
        if subscription_file_path not in files:
            files.append(subscription_file_path)
    else:
        # Inline XML import replaces manual or discovery-generated OPC UA subscriptions.
        config["subscriptions"] = subscriptions

    discovery = opcua.get("discovery")
    if isinstance(discovery, dict):
        discovery["enabled"] = False

    sink = config.setdefault("sink", {})
    routes = sink.setdefault("tag_prefix_routes", {})
    for prefix in sorted(route_prefixes):
        routes[prefix] = {"table": route_table}
    return config


def load_yaml(path: pathlib.Path) -> dict:
    with path.open("r", encoding="utf-8-sig") as file:
        data = yaml.safe_load(file)
    if not isinstance(data, dict):
        raise ValueError(f"{path} must contain a YAML object")
    return data


def write_yaml(path: pathlib.Path, data: dict) -> None:
    content = yaml.dump(
        data,
        Dumper=QuotedStringDumper,
        allow_unicode=True,
        sort_keys=False,
        default_flow_style=False,
    )
    path.write_text(content, encoding="utf-8", newline="\n")


def load_description_map(path: pathlib.Path) -> dict[str, str]:
    if not path.exists():
        return {}
    data = load_yaml(path)
    return {str(key): str(value) for key, value in data.items()}


def merge_description_maps(
    existing: dict[str, str],
    additions: dict[str, str],
    overwrite: bool = False,
) -> dict[str, str]:
    conflicts = [
        key
        for key, value in additions.items()
        if key in existing and existing[key] != value
    ]
    if conflicts and not overwrite:
        sample_key = sorted(conflicts)[0]
        raise ValueError(
            f"description conflict for {sample_key}; use --overwrite-descriptions to replace it"
        )

    merged = dict(existing)
    merged.update(additions)
    return dict(sorted(merged.items()))


def print_summary(document: AlarmPointDocument, subscriptions: list[dict], description_map_path: pathlib.Path) -> None:
    prefix_counts: dict[str, int] = {}
    for point in document.points:
        prefix = tag_prefix(point.tag_name)
        prefix_counts[prefix] = prefix_counts.get(prefix, 0) + 1

    print(f"points: {len(document.points)}")
    print(f"subscriptions: {len(subscriptions)}")
    print(f"description_map: {description_map_path}")
    print("prefix_counts:")
    for prefix, count in sorted(prefix_counts.items()):
        print(f"  {prefix}: {count}")


def parse_args():
    parser = argparse.ArgumentParser(description="Import OPC UA alarm points XML into Kepware Bridge config.")
    parser.add_argument("xml", help="OPC UA alarm point XML file")
    parser.add_argument("--config", default="config.local.yaml", help="YAML config to read/update")
    parser.add_argument(
        "--description-map",
        default=DEFAULT_DESCRIPTION_MAP_PATH,
        help="description map YAML path to write and reference from opcua.description_map_path",
    )
    parser.add_argument("--namespace-index", type=int, default=DEFAULT_NAMESPACE_INDEX)
    parser.add_argument(
        "--max-tags-per-subscription",
        type=int,
        default=DEFAULT_MAX_TAGS_PER_SUBSCRIPTION,
    )
    parser.add_argument("--subscription-prefix", default=DEFAULT_SUBSCRIPTION_PREFIX)
    parser.add_argument("--route-table", default=DEFAULT_ROUTE_TABLE)
    parser.add_argument(
        "--subscriptions-output",
        default=None,
        help="write subscriptions to a separate YAML file and reference it from opcua.subscription_files",
    )
    parser.add_argument(
        "--subscription-file-ref",
        default=None,
        help="subscription file path stored in config; defaults to --subscriptions-output",
    )
    parser.add_argument(
        "--merge-description-map",
        action="store_true",
        help="merge generated descriptions into an existing description map instead of replacing it",
    )
    parser.add_argument(
        "--overwrite-descriptions",
        action="store_true",
        help="replace conflicting existing description-map values when merging",
    )
    parser.add_argument(
        "--allow-empty-descriptions",
        action="store_true",
        help="allow points without Description and omit them from the description map",
    )
    parser.add_argument("--write", action="store_true", help="write config and description map")
    return parser.parse_args()


def run(args) -> int:
    xml_path = pathlib.Path(args.xml)
    config_path = pathlib.Path(args.config)
    description_map_path = pathlib.Path(args.description_map)
    subscriptions_output_path = (
        pathlib.Path(args.subscriptions_output) if args.subscriptions_output else None
    )
    subscription_file_ref = args.subscription_file_ref or args.subscriptions_output

    document = load_alarm_point_document(xml_path)
    validate_alarm_points(
        document,
        allow_empty_descriptions=args.allow_empty_descriptions,
    )
    subscriptions = build_subscriptions(
        document.points,
        max_tags_per_subscription=args.max_tags_per_subscription,
        namespace_index=args.namespace_index,
        subscription_prefix=args.subscription_prefix,
    )
    description_map = build_description_map(document.points, args.namespace_index)
    route_prefixes = {tag_prefix(point.tag_name) for point in document.points}

    print_summary(document, subscriptions, description_map_path)

    if not args.write:
        print("dry_run: true")
        return 0

    config = load_yaml(config_path)
    updated_config = update_config(
        config,
        subscriptions,
        route_prefixes,
        description_map_path=args.description_map,
        route_table=args.route_table,
        subscription_file_path=subscription_file_ref,
    )
    write_yaml(config_path, updated_config)
    if subscriptions_output_path is not None:
        write_yaml(subscriptions_output_path, {"subscriptions": subscriptions})
    if args.merge_description_map:
        description_map = merge_description_maps(
            load_description_map(description_map_path),
            description_map,
            overwrite=args.overwrite_descriptions,
        )
    write_yaml(description_map_path, description_map)
    print(f"updated_config: {config_path}")
    if subscriptions_output_path is not None:
        print(f"written_subscriptions: {subscriptions_output_path}")
    print(f"written_description_map: {description_map_path}")
    return 0


if __name__ == "__main__":
    sys.exit(run(parse_args()))
