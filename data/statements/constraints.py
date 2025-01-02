# If constraints.py is not present, but constraints.yaml is,
# then this file is automatically provided for booklet compilation.
#
# The script stores the entries of the two YAML files as global variables

import yaml
import sys

try:
    with open("constraints.yaml", "r") as constraints:
        constraints = yaml.safe_load(constraints)

        global_variables = globals()
        global_variables |= constraints
except FileNotFoundError:
    sys.stderr.write("No constraints.yaml file found")
