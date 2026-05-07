import importlib.util
import pathlib
import unittest

import yaml


MODULE_PATH = pathlib.Path(__file__).resolve().parents[1] / "scripts" / "opcua_browse_tags.py"
SPEC = importlib.util.spec_from_file_location("opcua_browse_tags", MODULE_PATH)
opcua_browse_tags = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(opcua_browse_tags)


class OpcuaBrowseTagsTests(unittest.TestCase):
    def test_collects_supported_scalar_user_tags_only(self):
        candidate = opcua_browse_tags.TagCandidate(
            node_id="ns=2;s=Channel.Device.Tag",
            browse_path=["Channel", "Device", "Tag"],
            data_type="Double",
            value_rank=-1,
        )

        self.assertTrue(
            opcua_browse_tags.should_collect_candidate(
                candidate,
                min_namespace=2,
                include_system=False,
                include_arrays=False,
                include_paths=[],
            )
        )

    def test_skips_system_statistics_arrays_and_unsupported_types(self):
        cases = [
            opcua_browse_tags.TagCandidate("ns=2;s=Channel.Device._System.Tag", ["Channel", "Device", "_System", "Tag"], "Double", -1),
            opcua_browse_tags.TagCandidate("ns=2;s=Channel._Statistics.Tag", ["Channel", "_Statistics", "Tag"], "UInt32", -1),
            opcua_browse_tags.TagCandidate("ns=2;s=_ThingWorx._StoredUpdateCount", ["_ThingWorx", "_StoredUpdateCount"], "UInt32", -1),
            opcua_browse_tags.TagCandidate("ns=2;s=Channel.Device.ArrayTag", ["Channel", "Device", "ArrayTag"], "Float", 1),
            opcua_browse_tags.TagCandidate("i=2258", ["Server", "CurrentTime"], "DateTime", -1),
        ]

        for candidate in cases:
            with self.subTest(candidate=candidate):
                self.assertFalse(
                    opcua_browse_tags.should_collect_candidate(
                        candidate,
                        min_namespace=2,
                        include_system=False,
                        include_arrays=False,
                        include_paths=[],
                    )
                )

    def test_include_paths_limit_collected_candidates_to_matching_branches(self):
        included = opcua_browse_tags.TagCandidate(
            "ns=2;s=数据类型示例.16 位设备.K 寄存器.Float1",
            ["数据类型示例", "16 位设备", "K 寄存器", "Float1"],
            "Float",
            -1,
        )
        excluded = opcua_browse_tags.TagCandidate(
            "ns=2;s=模拟器示例.函数.Random1",
            ["模拟器示例", "函数", "Random1"],
            "Int32",
            -1,
        )
        include_paths = [
            opcua_browse_tags.parse_include_path("数据类型示例.16 位设备.K 寄存器")
        ]

        self.assertTrue(
            opcua_browse_tags.should_collect_candidate(
                included,
                min_namespace=2,
                include_system=False,
                include_arrays=False,
                include_paths=include_paths,
            )
        )
        self.assertFalse(
            opcua_browse_tags.should_collect_candidate(
                excluded,
                min_namespace=2,
                include_system=False,
                include_arrays=False,
                include_paths=include_paths,
            )
        )

    def test_replaces_named_subscription_tags(self):
        config = yaml.safe_load(
            """
opcua:
  endpoint: "opc.tcp://127.0.0.1:49320"
subscriptions:
  - name: "fast"
    publishing_interval_ms: 500
    tags:
      - { node_id: "i=2259", alias: "server_state" }
  - name: "slow"
    publishing_interval_ms: 5000
    tags:
      - { node_id: "ns=2;s=Old", alias: "old" }
"""
        )
        tags = [
            {"node_id": "ns=2;s=Channel.Device.Tag1", "alias": "Channel_Device_Tag1"},
            {"node_id": "ns=2;s=Channel.Device.Tag2", "alias": "Channel_Device_Tag2"},
        ]

        updated = opcua_browse_tags.replace_subscription_tags(config, "fast", tags)

        self.assertEqual(updated["subscriptions"][0]["tags"], tags)
        self.assertEqual(updated["subscriptions"][1]["tags"], [{"node_id": "ns=2;s=Old", "alias": "old"}])


if __name__ == "__main__":
    unittest.main()
