#!/usr/bin/env python3
"""Small Snake game for MakOS/Raspberry Pi with Pygame."""

import random
import sys

import pygame


CELL = 20
GRID_WIDTH = 30
GRID_HEIGHT = 20
WIDTH = GRID_WIDTH * CELL
HEIGHT = GRID_HEIGHT * CELL
FPS = 12

BG = (18, 18, 24)
GRID = (32, 32, 42)
SNAKE = (80, 210, 120)
SNAKE_HEAD = (130, 245, 150)
FOOD = (245, 90, 90)
TEXT = (240, 240, 245)


def random_food(body):
    free = [
        (x, y)
        for x in range(GRID_WIDTH)
        for y in range(GRID_HEIGHT)
        if (x, y) not in body
    ]
    return random.choice(free) if free else None


def reset():
    body = [(GRID_WIDTH // 2, GRID_HEIGHT // 2)]
    return body, (1, 0), random_food(body), 0, False, False


def draw_text(screen, font, message, y):
    surface = font.render(message, True, TEXT)
    screen.blit(surface, surface.get_rect(center=(WIDTH // 2, y)))


def main():
    pygame.init()
    pygame.display.set_caption("MakOS Snake")
    screen = pygame.display.set_mode((WIDTH, HEIGHT))
    clock = pygame.time.Clock()
    font = pygame.font.Font(None, 30)
    large_font = pygame.font.Font(None, 52)

    body, direction, food, score, game_over, paused = reset()
    next_direction = direction
    running = True

    while running:
        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                running = False
            elif event.type == pygame.KEYDOWN:
                if event.key == pygame.K_ESCAPE:
                    running = False
                elif event.key == pygame.K_p and not game_over:
                    paused = not paused
                elif event.key == pygame.K_r:
                    body, direction, food, score, game_over, paused = reset()
                    next_direction = direction
                elif event.key in (pygame.K_UP, pygame.K_w) and direction != (0, 1):
                    next_direction = (0, -1)
                elif event.key in (pygame.K_DOWN, pygame.K_s) and direction != (0, -1):
                    next_direction = (0, 1)
                elif event.key in (pygame.K_LEFT, pygame.K_a) and direction != (1, 0):
                    next_direction = (-1, 0)
                elif event.key in (pygame.K_RIGHT, pygame.K_d) and direction != (-1, 0):
                    next_direction = (1, 0)

        if not paused and not game_over:
            direction = next_direction
            head_x, head_y = body[0]
            new_head = (head_x + direction[0], head_y + direction[1])
            eating = new_head == food
            collision_body = body if eating else body[:-1]

            if (
                not 0 <= new_head[0] < GRID_WIDTH
                or not 0 <= new_head[1] < GRID_HEIGHT
                or new_head in collision_body
            ):
                game_over = True
            else:
                body.insert(0, new_head)
                if eating:
                    score += 1
                    food = random_food(body)
                    if food is None:
                        game_over = True
                else:
                    body.pop()

        screen.fill(BG)
        for x in range(0, WIDTH, CELL):
            pygame.draw.line(screen, GRID, (x, 0), (x, HEIGHT))
        for y in range(0, HEIGHT, CELL):
            pygame.draw.line(screen, GRID, (0, y), (WIDTH, y))

        if food is not None:
            pygame.draw.rect(screen, FOOD, (food[0] * CELL, food[1] * CELL, CELL, CELL))
        for index, (x, y) in enumerate(body):
            color = SNAKE_HEAD if index == 0 else SNAKE
            pygame.draw.rect(screen, color, (x * CELL + 1, y * CELL + 1, CELL - 2, CELL - 2))

        score_surface = font.render(f"Score: {score}", True, TEXT)
        screen.blit(score_surface, (8, 6))
        if paused:
            draw_text(screen, large_font, "PAUSED", HEIGHT // 2 - 18)
            draw_text(screen, font, "Press P to continue", HEIGHT // 2 + 24)
        elif game_over:
            draw_text(screen, large_font, "GAME OVER", HEIGHT // 2 - 18)
            draw_text(screen, font, "Press R to restart", HEIGHT // 2 + 24)

        pygame.display.flip()
        clock.tick(FPS)

    pygame.quit()
    return 0


if __name__ == "__main__":
    sys.exit(main())
