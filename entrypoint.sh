#!/bin/sh

default_uid=$(id dex -u)
default_gid=$(id dex -g)

if [ ! "${UID}" = "${default_uid}" ] && [ -n "${UID}" ]; then
  usermod -o -u "${UID}" dex
fi

if [ ! "${GID}" = "${default_gid}" ] && [ -n "${GID}" ]; then
  groupmod -o -g "${GID}" dex
fi

exec runuser -u dex -- "${@}"
