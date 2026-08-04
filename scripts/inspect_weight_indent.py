from pathlib import Path

def main() -> None:
    path = Path(__file__).resolve().parents[1] / "examples" / "weight_tracker" / "weight_tracker_new.os"
    text = path.read_text()
    for i, line in enumerate(text.splitlines(), 1):
        if 105 <= i <= 115:
            leading = len(line) - len(line.lstrip(' '))
            print(f"{i:03}: {line!r}  leading_spaces={leading}")

if __name__ == "__main__":
    main()
