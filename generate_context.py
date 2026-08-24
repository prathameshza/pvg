import os

# Directories, subfolders, and files to exclude from context context
EXCLUDE_DIRS = {
    ".git",
    "target",
    "bin",
    "node_modules",
    ".vscode",
    "__pycache__",
}
EXCLUDE_FILES = {"Cargo.lock", "context.txt", "generate_context.py", ".DS_Store"}
EXCLUDE_EXTS = {".png", ".exe", ".dll", ".pdb", ".rlib", ".o", ".a", ".so"}

OUTPUT_FILE = "context.txt"


def generate_context(root_dir="."):
    with open(OUTPUT_FILE, "w", encoding="utf-8") as out:
        for current_path, dirs, files in os.walk(root_dir):
            # Modify dirs in-place to skip excluded directories during traversal
            dirs[:] = [d for d in dirs if d not in EXCLUDE_DIRS]

            for file in files:
                if file in EXCLUDE_FILES:
                    continue

                _, ext = os.path.splitext(file)
                if ext.lower() in EXCLUDE_EXTS:
                    continue

                file_path = os.path.join(current_path, file)
                relative_path = os.path.relpath(file_path, root_dir)

                out.write(f"=== File: {relative_path} ===\n")

                try:
                    with open(file_path, "r", encoding="utf-8") as f:
                        out.write(f.read())
                except Exception as e:
                    out.write(f"[Error reading file content: {e}]\n")

                out.write("\n\n" + "=" * 50 + "\n\n")

    print(f"Project context successfully generated in '{OUTPUT_FILE}'.")


if __name__ == "__main__":
    generate_context()