# Формули

Тестова нотатка з великою кількістю формул: рядкових `$…$` і блокових `$$…$$`, як їх пишуть в Obsidian.

## Основи

Енергія спокою $E = mc^2$, теорема Піфагора $a^2 + b^2 = c^2$, а корені квадратного рівняння дорівнюють $x_{1,2} = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}$.

Дроби $\frac{1}{2}$, $\tfrac{3}{4}$ і $\dfrac{a}{b}$, степені $e^{i\pi} + 1 = 0$, індекси $x_i$, $x_{i,j}$, $x^{2n+1}$, корені $\sqrt{2}$, $\sqrt[3]{x}$.

$$
x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
$$

## Суми, добутки, границі

Сума $\sum_{k=1}^{n} k = \frac{n(n+1)}{2}$ і добуток $\prod_{i=1}^{n} i = n!$ у тексті.

$$
\sum_{k=1}^{\infty} \frac{1}{k^2} = \frac{\pi^2}{6}
$$

$$
\lim_{x \to 0} \frac{\sin x}{x} = 1, \qquad \lim_{n \to \infty} \left(1 + \frac{1}{n}\right)^n = e
$$

$$
\prod_{p \text{ просте}} \frac{1}{1 - p^{-s}} = \zeta(s)
$$

## Інтеграли й похідні

Похідна $\frac{d}{dx} \sin x = \cos x$, частинна $\frac{\partial f}{\partial x}$, градієнт $\nabla f$.

$$
\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}
$$

$$
\oint_C \mathbf{F} \cdot d\mathbf{r} = \iint_S (\nabla \times \mathbf{F}) \cdot d\mathbf{S}
$$

$$
\int_a^b f(x)\,dx = F(b) - F(a)
$$

$$
\frac{\partial^2 u}{\partial t^2} = c^2 \nabla^2 u
$$

## Лінійна алгебра

Матриця $A \in \mathbb{R}^{n \times n}$, визначник $\det A$, транспонування $A^\top$, обернена $A^{-1}$.

$$
\begin{pmatrix} a & b \\ c & d \end{pmatrix}^{-1} = \frac{1}{ad - bc} \begin{pmatrix} d & -b \\ -c & a \end{pmatrix}
$$

$$
\begin{bmatrix} 1 & 0 & 0 \\ 0 & 1 & 0 \\ 0 & 0 & 1 \end{bmatrix}
\quad
\begin{vmatrix} a & b \\ c & d \end{vmatrix} = ad - bc
$$

$$
A\mathbf{x} = \lambda \mathbf{x}, \qquad \|\mathbf{x}\|_2 = \sqrt{\sum_i x_i^2}
$$

## Системи, випадки, вирівнювання

$$
f(x) = \begin{cases} x^2 & \text{якщо } x \ge 0 \\ -x & \text{інакше} \end{cases}
$$

$$
\begin{aligned}
(a + b)^2 &= a^2 + 2ab + b^2 \\
(a - b)^2 &= a^2 - 2ab + b^2 \\
a^2 - b^2 &= (a - b)(a + b)
\end{aligned}
$$

$$
\left\{
\begin{array}{l}
x + y = 5 \\
x - y = 1
\end{array}
\right.
$$

## Множини й логіка

Числові множини $\mathbb{N} \subset \mathbb{Z} \subset \mathbb{Q} \subset \mathbb{R} \subset \mathbb{C}$, порожня $\emptyset$, належність $x \in A$, $x \notin B$.

$$
A \cup B = \{x \mid x \in A \lor x \in B\}, \qquad A \cap B = \{x \mid x \in A \land x \in B\}
$$

$$
\forall \varepsilon > 0\ \exists \delta > 0 : |x - a| < \delta \Rightarrow |f(x) - f(a)| < \varepsilon
$$

$$
\neg (p \land q) \iff \neg p \lor \neg q
$$

## Грецькі літери й шрифти

$\alpha, \beta, \gamma, \delta, \epsilon, \zeta, \eta, \theta, \lambda, \mu, \pi, \rho, \sigma, \tau, \phi, \chi, \psi, \omega$ та $\Gamma, \Delta, \Theta, \Lambda, \Pi, \Sigma, \Phi, \Psi, \Omega$.

Шрифти: $\mathbb{R}$, $\mathcal{L}$, $\mathfrak{g}$, $\mathrm{d}x$, $\mathbf{v}$, $\mathit{text}$, $\mathsf{Set}$, $\mathtt{code}$.

## Акценти й дужки

$\hat{x}$, $\bar{x}$, $\tilde{x}$, $\vec{v}$, $\dot{x}$, $\ddot{x}$, $\overline{AB}$, $\underline{x}$, $\widehat{xyz}$.

$$
\left( \frac{a}{b} \right), \quad \left[ \frac{a}{b} \right], \quad \left\{ \frac{a}{b} \right\}, \quad \left| \frac{a}{b} \right|, \quad \langle \psi | \phi \rangle
$$

$$
\overbrace{a + b + c}^{\text{три}} = \underbrace{1 + 1 + 1}_{3}
$$

## Фізика

$$
\nabla \cdot \mathbf{E} = \frac{\rho}{\varepsilon_0}, \quad
\nabla \cdot \mathbf{B} = 0, \quad
\nabla \times \mathbf{E} = -\frac{\partial \mathbf{B}}{\partial t}, \quad
\nabla \times \mathbf{B} = \mu_0 \mathbf{J} + \mu_0 \varepsilon_0 \frac{\partial \mathbf{E}}{\partial t}
$$

$$
i\hbar \frac{\partial}{\partial t} \Psi(\mathbf{r}, t) = \hat{H} \Psi(\mathbf{r}, t)
$$

Рівняння руху $F = ma$, кінетична енергія $E_k = \frac{1}{2} m v^2$, закон тяжіння $F = G \frac{m_1 m_2}{r^2}$.

## Ймовірність і статистика

Біноміальний коефіцієнт $\binom{n}{k} = \frac{n!}{k!(n-k)!}$, сподівання $\mathbb{E}[X]$, дисперсія $\operatorname{Var}(X) = \mathbb{E}[X^2] - \mathbb{E}[X]^2$.

$$
f(x) = \frac{1}{\sigma \sqrt{2\pi}} \exp\left( -\frac{(x - \mu)^2}{2\sigma^2} \right)
$$

$$
P(A \mid B) = \frac{P(B \mid A)\, P(A)}{P(B)}
$$

## Оператори й символи

$\le, \ge, \ne, \approx, \equiv, \sim, \propto, \pm, \mp, \times, \div, \cdot, \circ, \infty, \partial, \nabla$.

Стрілки: $\to, \leftarrow, \Rightarrow, \Leftarrow, \Leftrightarrow, \mapsto, \uparrow, \downarrow, \longrightarrow$.

Функції: $\sin x$, $\cos x$, $\tan x$, $\log x$, $\ln x$, $\exp x$, $\max_i a_i$, $\min(a, b)$, $\arg\max_x f(x)$, $\gcd(a, b)$.

## Довгий запис

$$
\zeta(s) = \sum_{n=1}^{\infty} \frac{1}{n^s} = \frac{1}{1^s} + \frac{1}{2^s} + \frac{1}{3^s} + \cdots, \qquad \operatorname{Re}(s) > 1
$$

$$
\Gamma(z) = \int_0^\infty t^{z-1} e^{-t}\,dt, \qquad \Gamma(n + 1) = n!
$$

## Дужки й хімія

Скалярний добуток $\left\langle \psi \middle| \phi \right\rangle$ і реакція $\ce{2H2 + O2 -> 2H2O}$.

Кінець нотатки з формулами.
