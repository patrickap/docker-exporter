PROJECT_NAME := "patrickap/docker-exporter"
PROJECT_VERSION := "VERSION"

[private]
get_version:
  @cat {{PROJECT_VERSION}}

[private]
set_version version:
  @echo {{version}} > {{PROJECT_VERSION}}

[private]
backup_version:
  @cp {{PROJECT_VERSION}} {{PROJECT_VERSION}}.bak

[private]
restore_version:
  @cp {{PROJECT_VERSION}}.bak {{PROJECT_VERSION}}

[private]
docker_publish:
  @docker build --no-cache -t {{PROJECT_NAME}}:v$(just get_version) .				
  @docker push {{PROJECT_NAME}}:v$(just get_version)

[private]
git_publish:
  @git add . -- ':!{{PROJECT_VERSION}}.bak'
  @git commit -m "chore(release): v$(just get_version)"
  @git push
  @git tag -a "v$(just get_version)" -m "Release v$(just get_version)"
  @git push --tags origin

[private]
clean_up:
  @rm {{PROJECT_VERSION}}.bak

[private]
release_patch:
  @just backup_version
  @just set_version $(just get_version | awk -F. -v OFS=. '{$3++; print}')
  @just docker_publish && just git_publish || just restore_version
  @just clean_up

[private]
release_minor:
  @just backup_version
  @just set_version $(just get_version | awk -F. -v OFS=. '{$2++; $3=0; print}')
  @just docker_publish && just git_publish || just restore_version
  @just clean_up

[private]
release_major:
  @just backup_version
  @just set_version $(just get_version | awk -F. -v OFS=. '{$1++; $2=0; $3=0; print}')
  @just docker_publish && just git_publish || just restore_version
  @just clean_up

release type:
  @just release_{{type}}
