#!/bin/bash

set -e
shopt -s expand_aliases

export CONFIG="${CONFIG:-"conf.sh"}" SETOPT="help"

if [[ -f "${CONFIG}"  ]] ; then
	source "${CONFIG}"
fi

if [[ -f "bin/discord/discord.sh" ]] ; then
	alias discordsh='bin/discord/discord.sh'
else
	echo -e "requirement discord.sh not found. You can try:\n\t- git submodule init\n\t- git submodule update\nor:\n\t- git clone https://github.com/fieu/discord.sh.git \"bin/discord\""
	exit 1
fi

## Only long options allowed here.
while (( "${#}" > 0 )) ; do
	case "${1,,}" in
		("--push")
			shift
			export SETOPT="push"
		;;
		("--template")
			shift
			[[ -n "${1}" ]] && {
				export TEMPLATE="${1}"
				shift
			}
		;;
		(*)
			shift
		;;
	esac
done

case "${SETOPT:-help}" in
	("push")
		if [[ -f "${TEMPLATE}.conf" ]] ; then
			source "${TEMPLATE}.conf"
		elif [[ -f "${TEMPLATE}.sh.conf" ]] ; then
			source "${TEMPLATE}.sh.conf"
		elif [[ -f "${TEMPLATE}.conf.sh" ]] ; then
			source "${TEMPLATE}.conf.sh"
		fi

		if [[ -f "${TEMPLATE}" ]] ; then
			source "${TEMPLATE}"
		elif [[ -f "${TEMPLATE}.sh" ]] ; then
			source "${TEMPLATE}.sh"
		else
			echo "can't find template find \"${TEMPLATE:-NULL}\"."
			exit 1
		fi
	;;
	("help")
		echo "basic usage"
	;;
esac