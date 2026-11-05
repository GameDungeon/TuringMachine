import argparse
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed
from decimal import Decimal, InvalidOperation
from pathlib import Path

DEFAULT_STEPS = 10**6


def parse_steps(value):
    """Parse an integer or scientific notation into an integer."""
    try:
        number = Decimal(value)

        if not number.is_finite() or number != number.to_integral_value():
            raise ValueError

        result = int(number)

        if result < 1:
            raise ValueError

        return result

    except (InvalidOperation, ValueError):
        raise argparse.ArgumentTypeError(
            "steps must be a positive integer (e.g. 1000000 or 10e6)"
        )


def cargo_command(debug=False):
    command = ["cargo", "run", "-q"]

    if not debug:
        command.append("-r")

    return command


def run_machine(
    machine, steps=DEFAULT_STEPS, debug=False, show_output=False, output_path=None
):
    env = os.environ.copy()
    env["MACHINE"] = machine
    env["STEPS"] = str(steps)
    env["CARGO_BUILD_WARNINGS"] = "allow"

    if output_path is not None:
        env["OUTPUT_FILE"] = output_path

    try:
        result = subprocess.run(
            cargo_command(debug),
            env=env,
            stdout=None if show_output else subprocess.DEVNULL,
            stderr=None if show_output else subprocess.DEVNULL,
            check=False,
        )

        if result.returncode != 0:
            print(f"\nFAIL: {machine}")

        return result.returncode == 0

    except OSError as exc:
        print(f"\n[{machine}] Failed to execute Cargo: {exc}", file=sys.stderr)
        return False


def run_command(args):
    run_machine(
        args.machine,
        steps=args.steps,
        debug=args.debug,
        show_output=True,
    )


def load_machines(path):
    try:
        machines = path.read_text().splitlines()
    except OSError as exc:
        raise SystemExit(f"Cannot read {path}: {exc}")

    machines = [machine.strip() for machine in machines if machine.strip()]

    if not machines:
        raise SystemExit("No machines found.")

    return machines


def show_progress(completed, total):
    percent = completed * 100 // total
    filled = percent // 2
    bar = "#" * filled + " " * (50 - filled)

    print(
        f"\r[{bar}] {percent:3d}% ({completed}/{total})",
        end="",
        flush=True,
    )


def bulk_command(args):
    machines = load_machines(args.file)
    total = len(machines)

    args.output_file.parent.mkdir(parents=True, exist_ok=True)
    args.output_file.touch(exist_ok=True)
    args.output_file.write_text("# machine state symbol steps\n")

    print(f"Processing {total} machines using {args.jobs} workers.")
    print(f"Build mode: {'debug' if args.debug else 'release'}")
    print(f"Steps: {args.steps:,}")

    failed = []
    completed = 0

    show_progress(completed, total)

    try:
        with ThreadPoolExecutor(max_workers=args.jobs) as executor:
            futures = {
                executor.submit(
                    run_machine,
                    machine,
                    steps=args.steps,
                    debug=args.debug,
                    show_output=args.show_output,
                    output_path=str(args.output_file),
                ): machine
                for machine in machines
            }

            for future in as_completed(futures):
                futures[future]
                completed += 1
                show_progress(completed, total)

    except KeyboardInterrupt:
        print("\nInterrupted.", file=sys.stderr)
        return 130

    print("\n\nAll machines processed.")

    return 1 if failed else 0


def parse_args():
    parser = argparse.ArgumentParser(
        description="Run the Rust program against one or more machines.",
    )

    subparsers = parser.add_subparsers(dest="command")

    # Single machine command
    run_parser = subparsers.add_parser(
        "run",
        help="Run a single machine.",
    )

    run_parser.add_argument(
        "machine",
        help="Machine to run.",
    )

    run_parser.add_argument(
        "--steps",
        type=parse_steps,
        default=DEFAULT_STEPS,
        metavar="N",
        help="Number of steps (default: 1000000). Supports scientific notation.",
    )

    run_parser.add_argument(
        "--debug",
        action="store_true",
        help="Use a debug build instead of release.",
    )

    run_parser.set_defaults(func=run_command)

    # Bulk command
    bulk_parser = subparsers.add_parser(
        "bulk",
        help="Run multiple machines concurrently.",
    )

    bulk_parser.add_argument(
        "-f",
        "--file",
        type=Path,
        default=Path("machines.txt"),
        help="File containing one machine per line.",
    )

    bulk_parser.add_argument(
        "-j",
        "--jobs",
        type=int,
        default=os.cpu_count(),
        help="Maximum number of concurrent processes.",
    )

    bulk_parser.add_argument(
        "--steps",
        type=parse_steps,
        default=DEFAULT_STEPS,
        metavar="N",
        help="Number of steps (default: 1000000). Supports scientific notation.",
    )

    bulk_parser.add_argument(
        "--debug",
        "-d",
        action="store_true",
        help="Use debug builds instead of release.",
    )

    bulk_parser.add_argument(
        "--show-output",
        action="store_true",
        help="Show Cargo output from each process.",
    )

    bulk_parser.add_argument(
        "--output-file",
        type=Path,
        default=Path("out/machine_output.txt"),
        help="Output file to initialize.",
    )

    bulk_parser.set_defaults(func=bulk_command)

    args = parser.parse_args()

    if args.command is None:
        parser.print_help()
        sys.exit(0)

    if args.command == "bulk" and args.jobs < 1:
        parser.error("--jobs must be at least 1")

    return args


def main():
    args = parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
