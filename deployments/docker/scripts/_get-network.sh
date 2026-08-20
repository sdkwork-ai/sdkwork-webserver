#!/bin/bash
docker inspect postgres --format '{{json .NetworkSettings.Networks}}' | python3 -c 'import sys,json;d=json.load(sys.stdin);[print(k,v["IPAddress"]) for k,v in d.items()]'
echo "---redis---"
docker inspect redis --format '{{json .NetworkSettings.Networks}}' | python3 -c 'import sys,json;d=json.load(sys.stdin);[print(k,v["IPAddress"]) for k,v in d.items()]'
