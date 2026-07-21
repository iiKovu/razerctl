import importlib.machinery
import importlib.util
import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
loader = importlib.machinery.SourceFileLoader("blackshark_state", str(ROOT / "blackshark-state"))
spec = importlib.util.spec_from_loader(loader.name, loader)
state = importlib.util.module_from_spec(spec)
loader.exec_module(state)


class ResponseValidationTests(unittest.TestCase):
    def valid_battery_response(self):
        frame = bytearray(64)
        frame[0] = 0x02
        frame[1] = 0x02
        frame[2] = 0x60
        frame[6] = 5
        frame[9] = 0x80
        frame[10] = 0x21
        frame[11] = 0x01
        frame[12] = 0x01
        frame[13] = 31
        frame[14] = 0
        frame[62] = state.crc(frame)
        return bytes(frame)

    def test_accepts_checksum_correct_matching_battery_response(self):
        self.assertTrue(state.valid_response(self.valid_battery_response(), 0x21))

    def test_rejects_wrong_checksum_transaction_or_class(self):
        for index, value in [(62, 0), (2, 0x61), (10, 0x22)]:
            frame = bytearray(self.valid_battery_response())
            frame[index] = value
            self.assertFalse(state.valid_response(bytes(frame), 0x21))


if __name__ == "__main__":
    unittest.main()
