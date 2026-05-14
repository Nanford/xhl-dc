import importlib.util
import pathlib
import tempfile
import textwrap
import unittest

import yaml


MODULE_PATH = pathlib.Path(__file__).resolve().parents[1] / "scripts" / "import_opcua_alarm_points.py"
SPEC = importlib.util.spec_from_file_location("import_opcua_alarm_points", MODULE_PATH)
import_opcua_alarm_points = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(import_opcua_alarm_points)


def write_temp_xml(content: str) -> pathlib.Path:
    temp_dir = pathlib.Path(tempfile.mkdtemp())
    path = temp_dir / "alarm_points.xml"
    path.write_text(textwrap.dedent(content).strip(), encoding="utf-8")
    return path


class ImportOpcuaAlarmPointsTests(unittest.TestCase):
    def test_parses_xml_and_generates_tags_and_descriptions(self):
        xml_path = write_temp_xml(
            """
            <?xml version="1.0" encoding="utf-8"?>
            <OpcUaPointList totalCount="3">
              <Group name="输送设备" count="2">
                <Point id="1" sourceRow="2">
                  <TagName>WH_CP_Zone01.Convey.AreaControl.Alarm.Area1_B_Fault</TagName>
                  <Description>后超长</Description>
                </Point>
                <Point id="2" sourceRow="3">
                  <TagName>WH_CP_Zone01.Convey.AreaControl.Alarm.Area1_F_Fault</TagName>
                  <Description>前超长</Description>
                </Point>
              </Group>
              <Group name="范德兰德出库" count="1">
                <Point id="3" sourceRow="4">
                  <TagName>FSC1.OutBound.Iscs.BC-1_0_0-BC5191.MTR-1_0_0-BC5191_MTR.Details.DS</TagName>
                  <Description>电机状态：0、正常；1、错误；</Description>
                </Point>
              </Group>
            </OpcUaPointList>
            """
        )

        document = import_opcua_alarm_points.load_alarm_point_document(xml_path)
        import_opcua_alarm_points.validate_alarm_points(document)
        subscriptions = import_opcua_alarm_points.build_subscriptions(
            document.points,
            max_tags_per_subscription=2,
        )
        description_map = import_opcua_alarm_points.build_description_map(document.points)

        self.assertEqual(document.declared_total_count, 3)
        self.assertEqual(len(document.points), 3)
        self.assertEqual(
            subscriptions[0]["name"],
            "cpk_alarm_fsc1_001",
        )
        self.assertEqual(
            subscriptions[1]["name"],
            "cpk_alarm_wh_cp_zone01_001",
        )
        self.assertEqual(
            subscriptions[1]["tags"][0],
            {
                "node_id": "ns=2;s=WH_CP_Zone01.Convey.AreaControl.Alarm.Area1_B_Fault",
                "alias": "WH_CP_Zone01.Convey.AreaControl.Alarm.Area1_B_Fault",
            },
        )
        self.assertEqual(
            description_map["ns=2;s=FSC1.OutBound.Iscs.BC-1_0_0-BC5191.MTR-1_0_0-BC5191_MTR.Details.DS"],
            "电机状态：0、正常；1、错误；",
        )

    def test_parses_excel_export_xml_shape(self):
        xml_path = write_temp_xml(
            """
            <?xml version="1.0" encoding="utf-8"?>
            <OpcUaAlarmPoints>
              <Metadata>
                <GeneratedAt>2026-05-14 05:54:47</GeneratedAt>
                <SourceType>Excel</SourceType>
              </Metadata>
              <Summary>
                <SourceCount name="HCK_Convey" count="2" />
                <SourceCount name="GJK_Convey" count="1" />
                <TotalCount>3</TotalCount>
              </Summary>
              <Sources>
                <Source name="HCK_Convey" fileName="HCK_Convey.xlsx" sheetName="HCK_Convey">
                  <Points>
                    <Point sourceRow="2">
                      <TagName>WH_FLK_Zone01.HCK_Convey.Conveyor.3202.Alarm.BlFault</TagName>
                      <Description>后超长</Description>
                    </Point>
                    <Point sourceRow="3">
                      <TagName>WH_FLK_Zone01.HCK_Convey.Conveyor.3202.Alarm.BwOverlimit</TagName>
                      <Description>后超限</Description>
                    </Point>
                  </Points>
                </Source>
                <Source name="GJK_Convey" fileName="GJK_Convey.xlsx" sheetName="GJK_Convey">
                  <Points>
                    <Point sourceRow="2">
                      <TagName>WH_FLK_Zone01.GJK_Convey.Conveyor.3001.Alarm.BFault</TagName>
                      <Description>后超长</Description>
                    </Point>
                  </Points>
                </Source>
              </Sources>
            </OpcUaAlarmPoints>
            """
        )

        document = import_opcua_alarm_points.load_alarm_point_document(xml_path)
        import_opcua_alarm_points.validate_alarm_points(document)
        subscriptions = import_opcua_alarm_points.build_subscriptions(
            document.points,
            max_tags_per_subscription=2,
            subscription_prefix="flk_alarm",
        )

        self.assertEqual(document.declared_total_count, 3)
        self.assertEqual(len(document.points), 3)
        self.assertEqual(
            [subscription["name"] for subscription in subscriptions],
            [
                "flk_alarm_wh_flk_zone01_001",
                "flk_alarm_wh_flk_zone01_002",
            ],
        )
        self.assertEqual(document.points[0].group_name, "HCK_Convey")
        self.assertEqual(document.points[2].group_name, "GJK_Convey")

    def test_rejects_empty_duplicate_and_total_mismatch(self):
        xml_path = write_temp_xml(
            """
            <?xml version="1.0" encoding="utf-8"?>
            <OpcUaPointList totalCount="4">
              <Group name="输送设备" count="3">
                <Point id="1" sourceRow="2">
                  <TagName>WH_CP_Zone01.Alarm.A</TagName>
                  <Description>故障A</Description>
                </Point>
                <Point id="2" sourceRow="3">
                  <TagName>WH_CP_Zone01.Alarm.A</TagName>
                  <Description>故障A重复</Description>
                </Point>
                <Point id="3" sourceRow="4">
                  <TagName></TagName>
                  <Description>空点位</Description>
                </Point>
              </Group>
            </OpcUaPointList>
            """
        )

        document = import_opcua_alarm_points.load_alarm_point_document(xml_path)

        with self.assertRaisesRegex(ValueError, "declared totalCount 4 does not match 3 points"):
            import_opcua_alarm_points.validate_alarm_points(document)

        document = import_opcua_alarm_points.AlarmPointDocument(
            declared_total_count=3,
            points=document.points,
        )
        with self.assertRaisesRegex(ValueError, "empty TagName"):
            import_opcua_alarm_points.validate_alarm_points(document)

    def test_splits_by_prefix_and_updates_config_routes(self):
        points = [
            import_opcua_alarm_points.AlarmPoint("WH_CP_Zone02.Alarm.A", "故障A"),
            import_opcua_alarm_points.AlarmPoint("WH_CP_Zone02.Alarm.B", "故障B"),
            import_opcua_alarm_points.AlarmPoint("HCC02.Alarm.Inbound_Task_Timeout", "入库环穿任务超时"),
        ]
        config = yaml.safe_load(
            """
            opcua:
              endpoint: "opc.tcp://127.0.0.1:49320"
              discovery:
                enabled: true
                target_subscription: "old"
            subscriptions:
              - name: "old"
                publishing_interval_ms: 500
                keep_alive_count: 10
                lifetime_count: 30
                tags:
                  - { node_id: "ns=2;s=Old", alias: "Old" }
            sink:
              table: "ylk_alarm_log"
              tag_prefix_routes:
                YLK1:
                  table: "ylk_alarm_log"
            """
        )

        subscriptions = import_opcua_alarm_points.build_subscriptions(
            points,
            max_tags_per_subscription=1,
        )
        updated = import_opcua_alarm_points.update_config(
            config,
            subscriptions,
            {"WH_CP_Zone02", "HCC02"},
            description_map_path="./description_map.cpk.yaml",
        )

        self.assertEqual(
            [subscription["name"] for subscription in updated["subscriptions"]],
            [
                "cpk_alarm_hcc02_001",
                "cpk_alarm_wh_cp_zone02_001",
                "cpk_alarm_wh_cp_zone02_002",
            ],
        )
        self.assertEqual(
            updated["sink"]["tag_prefix_routes"]["WH_CP_Zone02"],
            {"table": "cpk_alarm_log"},
        )
        self.assertEqual(
            updated["sink"]["tag_prefix_routes"]["HCC02"],
            {"table": "cpk_alarm_log"},
        )
        self.assertEqual(
            updated["sink"]["tag_prefix_routes"]["YLK1"],
            {"table": "ylk_alarm_log"},
        )
        self.assertEqual(updated["opcua"]["description_map_path"], "./description_map.cpk.yaml")
        self.assertEqual(updated["opcua"]["monitored_item_create_batch_size_count"], 500)
        self.assertFalse(updated["opcua"]["discovery"]["enabled"])

    def test_updates_config_with_external_subscription_file_reference(self):
        config = yaml.safe_load(
            """
            opcua:
              endpoint: "opc.tcp://127.0.0.1:49320"
              subscription_files:
                - "./points/subscriptions.cpk.yaml"
            subscriptions:
              - name: "manual"
                publishing_interval_ms: 500
                keep_alive_count: 10
                lifetime_count: 30
                tags:
                  - { node_id: "ns=2;s=Manual", alias: "Manual" }
            sink:
              table: "ylk_alarm_log"
              tag_prefix_routes: {}
            """
        )

        updated = import_opcua_alarm_points.update_config(
            config,
            [{"name": "flk_alarm_wh_flk_zone01_001", "tags": []}],
            {"WH_FLK_Zone01"},
            description_map_path="./points/descriptions.yaml",
            route_table="flk_alarm_log",
            subscription_file_path="./points/subscriptions.flk.yaml",
        )

        self.assertEqual(
            updated["opcua"]["subscription_files"],
            [
                "./points/subscriptions.cpk.yaml",
                "./points/subscriptions.flk.yaml",
            ],
        )
        self.assertEqual([item["name"] for item in updated["subscriptions"]], ["manual"])
        self.assertEqual(
            updated["sink"]["tag_prefix_routes"]["WH_FLK_Zone01"],
            {"table": "flk_alarm_log"},
        )

    def test_write_yaml_quotes_mysql_urls_for_serde_yaml_compatibility(self):
        temp_dir = pathlib.Path(tempfile.mkdtemp())
        output = temp_dir / "config.yaml"

        import_opcua_alarm_points.write_yaml(
            output,
            {"mysql": {"url": "mysql://user:sample%40value@127.0.0.1:3306/iot"}},
        )

        content = output.read_text(encoding="utf-8")

        self.assertIn('"url": "mysql://user:sample%40value@127.0.0.1:3306/iot"', content)
        self.assertEqual(
            yaml.safe_load(content)["mysql"]["url"],
            "mysql://user:sample%40value@127.0.0.1:3306/iot",
        )


if __name__ == "__main__":
    unittest.main()
