#!/bin/sh

echo "Starting container"

default_uid=$(id dex -u)
default_gid=$(id dex -g)

if [ ! "${UID}" = "${default_uid}" ] && [ -n "${UID}" ]; then
  echo "Changing UID from '${default_uid}' to '${UID}'"
  usermod -o -u "${UID}" dex
fi

if [ ! "${GID}" = "${default_gid}" ] && [ -n "${GID}" ]; then
  echo "Changing GID from '${default_gid}' to '${GID}'"
  groupmod -o -g "${GID}" dex
fi

echo "Running container as $(id dex)"
exec su-exec dex "${@}"
