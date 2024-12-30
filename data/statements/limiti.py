# If limiti.py is not present, but limiti.yaml is,
# then this file is automatically provided for booklet compilation.
#
# The script stores the entries of the two YAML files as global variables

import yaml
import sys

try:
    with open("limiti.yaml", "r") as constraints:
        constraints = yaml.safe_load(constraints)

        global_variables = globals()
        global_variables |= constraints
except FileNotFoundError:
    sys.stderr.write("No limiti.yaml file found")
