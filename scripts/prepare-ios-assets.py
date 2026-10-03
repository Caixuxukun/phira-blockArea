"""Restore only omitted runtime assets from the pinned official Phira release."""
import argparse
import hashlib
from pathlib import Path, PurePosixPath
import tempfile
import urllib.request
import zipfile

URL = "https://github.com/TeamFlos/phira/releases/download/v0.8.2/Phira-linux-x86_64-v0.8.2.zip"
SHA256 = "63efb25aadcd7a30a968572f4ac68d3e8b327be793440e21dca432be8071965c"
ROOT = Path(__file__).resolve().parents[1]


def restore(archive):
    if hashlib.sha256(archive.read_bytes()).hexdigest() != SHA256:
        raise ValueError("Official asset archive SHA-256 mismatch")
    with zipfile.ZipFile(archive) as bundle:
        for entry in bundle.infolist():
            name = PurePosixPath(entry.filename)
            if entry.is_dir() or entry.filename not in ("assets/font.ttf", "assets/background.jpg") and not entry.filename.startswith("assets/res/"):
                continue
            if name.is_absolute() or ".." in name.parts or "\\" in entry.filename:
                raise ValueError("Unsafe archive path")
            destination = ROOT.joinpath(*name.parts)
            if not destination.exists():
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(bundle.read(entry))
    for name in ("assets/font.ttf", "assets/background.jpg"):
        if not (ROOT / name).is_file() or (ROOT / name).stat().st_size == 0:
            raise ValueError(f"Missing runtime asset: {name}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", type=Path, help="Use an already downloaded, checksum-verified release zip")
    args = parser.parse_args()
    if args.archive:
        restore(args.archive)
    else:
        with tempfile.TemporaryDirectory(prefix="phira-ios-assets-") as temporary:
            archive = Path(temporary) / "assets.zip"
            urllib.request.urlretrieve(URL, archive)
            restore(archive)
