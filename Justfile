PROJECT_NAME := "patrickap/docker-exporter"
PROJECT_VERSION := "VERSION"

[private]
get-version:
  @cat {{PROJECT_VERSION}}

[private]
set-version version:
  @echo {{version}} > {{PROJECT_VERSION}}

[private]
backup-version:
  @cp {{PROJECT_VERSION}} {{PROJECT_VERSION}}.bak

[private]
restore-version:
  @cp {{PROJECT_VERSION}}.bak {{PROJECT_VERSION}}

[private]
docker-publish:
  @docker buildx build --platform linux/amd64,linux/arm64 --no-cache -t {{PROJECT_NAME}}:v$(just get-version) .	
  @docker push {{PROJECT_NAME}}:v$(just get-version)

[private]
git-publish:
  @git add . -- ':!{{PROJECT_VERSION}}.bak'
  @git commit -m "chore(release): v$(just get-version)"
  @git push
  @git tag -a "v$(just get-version)" -m "Release v$(just get-version)"
  @git push --tags origin

[private]
clean-up:
  @rm {{PROJECT_VERSION}}.bak

[private]
release-patch:
  @just backup-version
  @just set-version $(just get-version | awk -F. -v OFS=. '{$3++; print}')
  @just docker-publish && just git-publish || just restore-version
  @just clean-up

[private]
release-minor:
  @just backup-version
  @just set-version $(just get-version | awk -F. -v OFS=. '{$2++; $3=0; print}')
  @just docker-publish && just git-publish || just restore-version
  @just clean-up

[private]
release-major:
  @just backup-version
  @just set-version $(just get-version | awk -F. -v OFS=. '{$1++; $2=0; $3=0; print}')
  @just docker-publish && just git-publish || just restore-version
  @just clean-up

release type:
  @just release-{{type}}
